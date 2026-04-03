#!/usr/bin/env python3
"""
Wakey Voice Plugin - LiveKit Agent

This plugin runs as a subprocess spawned by Wakey. It:
- Reads JSON events from stdin (ShouldSpeak, Shutdown)
- Writes JSON events to stdout (VoiceListeningStarted, VoiceUserSpeaking, etc.)
- Connects to LiveKit room as an agent worker for audio processing

Communication Protocol (JSON lines):

Wakey → Plugin:
    {"event": "ShouldSpeak", "text": "Hi there!", "urgency": "low"}
    {"event": "Shutdown"}

Plugin → Wakey:
    {"event": "VoiceListeningStarted"}
    {"event": "VoiceUserSpeaking", "text": "hello", "is_final": false}
    {"event": "VoiceUserSpeaking", "text": "hello wakey", "is_final": true}
    {"event": "VoiceWakeyThinking"}
    {"event": "VoiceWakeySpeaking", "text": "Hey! What's up?"}
    {"event": "VoiceSessionEnded"}
    {"event": "VoiceError", "message": "..."}
"""

import asyncio
import json
import logging
import os
import sys
import threading
from dataclasses import dataclass, field
from typing import Any, AsyncIterator, Optional

from livekit.agents import (
    Agent,
    AgentSession,
    JobContext,
    JobProcess,
    WorkerOptions,
    cli,
    stt,
    tts,
    llm,
)
from livekit.agents.stt import SpeechEventType, SpeechEvent
from livekit.plugins import deepgram, silero, openai
from livekit.plugins.turn_detector.multilingual import MultilingualModel

# Configure logging to stderr (stdout is for events only)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    stream=sys.stderr,
)
logger = logging.getLogger("wakey-voice-plugin")

# Environment variables required
LIVEKIT_URL = os.getenv("LIVEKIT_URL")
LIVEKIT_API_KEY = os.getenv("LIVEKIT_API_KEY")
LIVEKIT_API_SECRET = os.getenv("LIVEKIT_API_SECRET")
DEEPGRAM_API_KEY = os.getenv("DEEPGRAM_API_KEY")
GROQ_API_KEY = os.getenv("GROQ_API_KEY")


class StdoutEventEmitter:
    """Thread-safe JSON event writer to stdout."""
    
    def __init__(self):
        self._lock = threading.Lock()
    
    def emit(self, event: str, **kwargs):
        """Write a JSON event line to stdout."""
        payload = {"event": event}
        payload.update(kwargs)
        with self._lock:
            try:
                sys.stdout.write(json.dumps(payload) + "\n")
                sys.stdout.flush()
            except Exception as e:
                logger.error(f"Failed to emit event: {e}")


# Global event emitter
emitter = StdoutEventEmitter()


class EventEmittingSTT(stt.STT):
    """
    STT wrapper that emits VoiceUserSpeaking events.
    
    Wraps the underlying STT plugin and intercepts speech events
    to emit them to stdout before passing them through.
    """
    
    def __init__(self, base_stt: stt.STT):
        self._base_stt = base_stt
        super().__init__(
            streaming=base_stt.streaming,
            capabilities=base_stt.capabilities,
        )
    
    async def recognize(
        self,
        buffer: Any,
        *,
        language: Optional[str] = None,
    ) -> stt.SpeechEvent:
        """Non-streaming recognition (single utterance)."""
        result = await self._base_stt.recognize(buffer, language=language)
        
        # Emit final transcript
        if result.type == SpeechEventType.FINAL_TRANSCRIPT:
            text = result.alternatives[0].text if result.alternatives else ""
            if text:
                emitter.emit("VoiceUserSpeaking", text=text, is_final=True)
                logger.info(f"User said (final): {text}")
        
        return result
    
    async def stream(self, language: Optional[str] = None) -> AsyncIterator[SpeechEvent]:
        """Streaming recognition with event emission."""
        async for event in self._base_stt.stream(language=language):
            # Emit interim and final transcripts
            if event.type == SpeechEventType.INTERIM_TRANSCRIPT:
                text = event.alternatives[0].text if event.alternatives else ""
                if text:
                    emitter.emit("VoiceUserSpeaking", text=text, is_final=False)
                    logger.info(f"User saying (interim): {text}")
            
            elif event.type == SpeechEventType.FINAL_TRANSCRIPT:
                text = event.alternatives[0].text if event.alternatives else ""
                if text:
                    emitter.emit("VoiceUserSpeaking", text=text, is_final=True)
                    logger.info(f"User said (final): {text}")
            
            yield event


class EventEmittingLLM(llm.LLM):
    """
    LLM wrapper that emits VoiceWakeyThinking events.
    
    Emits event when LLM generation starts, then passes through
    the response stream.
    """
    
    def __init__(self, base_llm: llm.LLM):
        self._base_llm = base_llm
        super().__init__()
    
    async def chat(
        self,
        messages: list[llm.ChatMessage],
        *,
        tools: Optional[list[llm.Tool]] = None,
        **kwargs,
    ) -> AsyncIterator[llm.ChatChunk]:
        """Chat with event emission on start."""
        # Emit thinking event when LLM is called
        emitter.emit("VoiceWakeyThinking")
        logger.info("Agent thinking (LLM call started)")
        
        # Stream through base LLM
        async for chunk in self._base_llm.chat(messages, tools=tools, **kwargs):
            yield chunk


class EventEmittingTTS(tts.TTS):
    """
    TTS wrapper that emits VoiceWakeySpeaking events.
    
    Emits event when text is being synthesized to speech.
    """
    
    def __init__(self, base_tts: tts.TTS):
        self._base_tts = base_tts
        super().__init__(
            streaming=base_tts.streaming,
            capabilities=base_tts.capabilities,
        )
    
    async def synthesize(
        self,
        text: str,
        *,
        language: Optional[str] = None,
        voice: Optional[str] = None,
    ) -> tts.SynthesizedAudio:
        """Non-streaming synthesis with event emission."""
        # Emit speaking event
        emitter.emit("VoiceWakeySpeaking", text=text)
        logger.info(f"Agent speaking: {text}")
        
        return await self._base_tts.synthesize(text, language=language, voice=voice)
    
    async def stream(self, *, language: Optional[str] = None, voice: Optional[str] = None) -> tts.TTSStream:
        """Streaming synthesis - wrapped at stream level."""
        # Create wrapped stream
        base_stream = await self._base_tts.stream(language=language, voice=voice)
        return EventEmittingTTSStream(base_stream)


class EventEmittingTTSStream(tts.TTSStream):
    """TTS stream wrapper that tracks text being synthesized."""
    
    def __init__(self, base_stream: tts.TTSStream):
        self._base_stream = base_stream
        self._current_text = ""
    
    async def send_text(self, text: str) -> None:
        """Send text chunk and emit event."""
        self._current_text += text
        
        # Emit speaking event for text chunk
        if text:
            emitter.emit("VoiceWakeySpeaking", text=text)
            logger.info(f"Agent speaking chunk: {text}")
        
        await self._base_stream.send_text(text)
    
    def end_input(self) -> None:
        """End input stream."""
        self._base_stream.end_input()
    
    async def __anext__(self) -> tts.SynthesizedAudio:
        """Get next audio chunk."""
        return await self._base_stream.__anext__()
    
    async def aclose(self) -> None:
        """Close stream."""
        await self._base_stream.aclose()


@dataclass
class PluginState:
    """Shared state between stdin reader and agent."""
    session: Optional[AgentSession] = None
    should_speak_queue: asyncio.Queue = field(default_factory=asyncio.Queue)
    running: bool = True
    shutdown_requested: bool = False
    connected: bool = False


# Global state
state = PluginState()


class WakeyAgent(Agent):
    """Wakey's voice agent."""
    
    def __init__(self):
        super().__init__(
            instructions="""You are Wakey, a friendly AI companion that lives on the user's laptop.
You're warm, helpful, and occasionally playful. Keep responses concise and natural.
You can see what the user is doing on their screen and remember past conversations.
When asked to help, be proactive and specific. Speak like a friend, not a bot."""
        )
    
    async def on_enter(self) -> None:
        """Called when agent enters the LiveKit room."""
        emitter.emit("VoiceListeningStarted")
        state.connected = True
        logger.info("Agent entered room, listening started")
    
    async def on_exit(self) -> None:
        """Called when agent exits the LiveKit room."""
        emitter.emit("VoiceSessionEnded")
        state.connected = False
        logger.info("Agent exited room, session ended")


async def process_should_speak_queue():
    """Process ShouldSpeak events from the queue."""
    while state.running:
        try:
            # Wait for ShouldSpeak event
            item = await state.should_speak_queue.get()
            
            if item is None:  # Shutdown signal
                break
            
            text, urgency = item
            
            if state.session and state.connected:
                logger.info(f"Processing ShouldSpeak: '{text}' (urgency={urgency})")
                
                # Generate reply
                await state.session.generate_reply(
                    instructions=f"Say this to the user: {text}",
                )
            else:
                logger.warning("ShouldSpeak received but session not connected")
        
        except asyncio.CancelledError:
            break
        except Exception as e:
            logger.error(f"Error processing ShouldSpeak: {e}")
            emitter.emit("VoiceError", message=str(e))


async def stdin_reader():
    """Async task to read JSON events from stdin."""
    logger.info("Stdin reader started")
    loop = asyncio.get_event_loop()
    
    while state.running:
        try:
            # Read line from stdin
            line = await loop.run_in_executor(None, sys.stdin.readline)
            
            if not line:
                logger.info("Stdin EOF, shutting down")
                state.shutdown_requested = True
                state.running = False
                await state.should_speak_queue.put(None)
                break
            
            line = line.strip()
            if not line:
                continue
            
            try:
                data = json.loads(line)
                event_type = data.get("event")
                
                if event_type == "ShouldSpeak":
                    text = data.get("text", "")
                    urgency = data.get("urgency", "low")
                    await state.should_speak_queue.put((text, urgency))
                
                elif event_type == "Shutdown":
                    logger.info("Shutdown event received")
                    state.shutdown_requested = True
                    state.running = False
                    await state.should_speak_queue.put(None)
                    break
                
                else:
                    logger.warning(f"Unknown event: {event_type}")
            
            except json.JSONDecodeError as e:
                logger.error(f"Invalid JSON: {line}")
        
        except Exception as e:
            logger.error(f"Stdin reader error: {e}")
            emitter.emit("VoiceError", message=str(e))
            break
    
    logger.info("Stdin reader stopped")


def prewarm(proc: JobProcess):
    """Prewarm: Load VAD model before job starts."""
    proc.userdata["vad"] = silero.VAD.load()
    logger.info("VAD model loaded")


async def entrypoint(ctx: JobContext):
    """Main entrypoint: Connect to LiveKit room and start agent session."""
    
    # Validate environment
    if not all([LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET]):
        emitter.emit("VoiceError", message="Missing LiveKit credentials")
        logger.error("Missing LiveKit environment variables")
        return
    
    if not DEEPGRAM_API_KEY:
        emitter.emit("VoiceError", message="Missing Deepgram API key")
        logger.error("Missing DEEPGRAM_API_KEY")
        return
    
    if not GROQ_API_KEY:
        emitter.emit("VoiceError", message="Missing Groq API key")
        logger.error("Missing GROQ_API_KEY")
        return
    
    logger.info(f"Connecting to LiveKit room: {ctx.room.name}")
    await ctx.connect()
    
    # Create wrapped plugins with event emission
    base_stt = deepgram.STT(model="nova-3", language="en")
    base_llm = openai.LLM(
        model="llama-3.3-70b-versatile",
        base_url="https://api.groq.com/openai/v1",
        api_key=GROQ_API_KEY,
    )
    base_tts = deepgram.TTS(model="aura-2-asteria-en")
    
    # Wrap with event-emitting versions
    wrapped_stt = EventEmittingSTT(base_stt)
    wrapped_llm = EventEmittingLLM(base_llm)
    wrapped_tts = EventEmittingTTS(base_tts)
    
    # Create agent session
    session = AgentSession(
        stt=wrapped_stt,
        llm=wrapped_llm,
        tts=wrapped_tts,
        vad=ctx.proc.userdata["vad"],
        turn_detection=MultilingualModel(),
        allow_interruptions=True,
        min_words_to_interrupt=2,
    )
    
    state.session = session
    agent = WakeyAgent()
    
    # Start background tasks
    stdin_task = asyncio.create_task(stdin_reader())
    queue_task = asyncio.create_task(process_should_speak_queue())
    
    # Start agent session
    logger.info("Starting agent session...")
    await session.start(agent=agent, room=ctx.room)
    
    # Keep running until shutdown
    try:
        while state.running and not state.shutdown_requested:
            await asyncio.sleep(0.5)
        
        logger.info("Shutting down...")
        emitter.emit("VoiceSessionEnded")
    
    except asyncio.CancelledError:
        logger.info("Job cancelled")
        emitter.emit("VoiceSessionEnded")
    
    finally:
        stdin_task.cancel()
        queue_task.cancel()
        
        for task in [stdin_task, queue_task]:
            try:
                await task
            except asyncio.CancelledError:
                pass
        
        if session:
            await session.aclose()
        
        state.session = None
        logger.info("Session closed")


def main():
    """Main entry point for the plugin."""
    logger.info("Wakey Voice Plugin (LiveKit) starting...")
    
    # Validate credentials
    missing = []
    for var_name in ["LIVEKIT_URL", "LIVEKIT_API_KEY", "LIVEKIT_API_SECRET", 
                     "DEEPGRAM_API_KEY", "GROQ_API_KEY"]:
        if not os.getenv(var_name):
            missing.append(var_name)
    
    if missing:
        emitter.emit("VoiceError", message=f"Missing env vars: {missing}")
        logger.error(f"Missing required env vars: {missing}")
        sys.exit(1)
    
    cli.run_app(WorkerOptions(entrypoint_fnc=entrypoint, prewarm_fnc=prewarm))


if __name__ == "__main__":
    main()