#!/usr/bin/env python3
"""
Wakey Voice Plugin — LiveKit Agent

Subprocess spawned by Wakey. Communicates via JSON lines on stdin/stdout.
"""

import json
import os
import sys
import threading
import logging

# Silence logs to stderr (stdout is for JSON protocol only)
logging.basicConfig(level=logging.WARNING, stream=sys.stderr)


def emit(event: str, **kwargs):
    """Send JSON event to Wakey via stdout."""
    msg = {"event": event, **kwargs}
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


# Read stdin for ShouldSpeak events in a background thread
def stdin_reader():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
            event = msg.get("event", "")
            if event == "Shutdown":
                emit("VoiceSessionEnded")
                os._exit(0)
        except json.JSONDecodeError:
            pass


stdin_thread = threading.Thread(target=stdin_reader, daemon=True)
stdin_thread.start()

# Emit ready
emit("VoiceListeningStarted")

# --- LiveKit Agent ---

from livekit import agents, rtc
from livekit.agents import AgentServer, AgentSession, Agent, TurnHandlingOptions
from livekit.plugins import silero

groq_key = os.environ.get("GROQ_API_KEY", "")


class WakeyAgent(Agent):
    def __init__(self):
        super().__init__(
            instructions="""You are Wakey, a friendly voice companion that lives on the user's desktop.
You're warm, curious, and a little playful. Keep responses conversational and concise.
This is a voice conversation — avoid long paragraphs, formatting, emojis, or symbols.
Be yourself. Just be Wakey.""",
        )


server = AgentServer()


@server.rtc_session(agent_name="wakey-voice")
async def wakey_session(ctx: agents.JobContext):
    # Build LLM config
    if groq_key:
        from livekit.plugins import openai as lk_openai
        llm = lk_openai.LLM.with_groq(model="openai/gpt-oss-20b")
    else:
        llm = "openai/gpt-4.1-mini"

    session = AgentSession(
        stt="deepgram/nova-3:multi",
        llm=llm,
        tts="deepgram/aura-2-theia-en",
        vad=silero.VAD.load(),
        turn_handling=TurnHandlingOptions(turn_detection="vad"),
    )

    @session.on("user_input_transcribed")
    def on_transcript(transcript):
        emit("VoiceUserSpeaking", text=transcript.transcript, is_final=transcript.is_final)

    @session.on("agent_started_speaking")
    def on_speaking():
        emit("VoiceWakeySpeaking", text="")

    @session.on("agent_stopped_speaking")
    def on_stopped():
        emit("VoiceSessionEnded")

    await session.start(room=ctx.room, agent=WakeyAgent())
    await session.generate_reply(instructions="Greet the user warmly.")


if __name__ == "__main__":
    sys.argv = [sys.argv[0], "dev"]
    agents.cli.run_app(server)
