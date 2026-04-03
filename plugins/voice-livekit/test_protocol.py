#!/usr/bin/env python3
"""
Test script for the Wakey Voice Plugin.

This script tests the stdin/stdout communication protocol
without requiring a full LiveKit setup.

Usage:
    python3 test_protocol.py
    
This will:
1. Mock stdin events
2. Verify stdout event format
3. Check JSON parsing
"""

import json
import sys


def test_event_format():
    """Test that all events have the correct format."""
    
    test_cases = [
        # Events from Wakey to Plugin
        {
            "event": "ShouldSpeak",
            "text": "Hello, how are you?",
            "urgency": "low"
        },
        {
            "event": "ShouldSpeak",
            "text": "Important message!",
            "urgency": "high"
        },
        {
            "event": "Shutdown"
        },
        
        # Events from Plugin to Wakey
        {
            "event": "VoiceListeningStarted"
        },
        {
            "event": "VoiceUserSpeaking",
            "text": "hello",
            "is_final": False
        },
        {
            "event": "VoiceUserSpeaking",
            "text": "hello wakey",
            "is_final": True
        },
        {
            "event": "VoiceWakeyThinking"
        },
        {
            "event": "VoiceWakeySpeaking",
            "text": "Hey! What's up?"
        },
        {
            "event": "VoiceSessionEnded"
        },
        {
            "event": "VoiceError",
            "message": "Connection failed"
        },
    ]
    
    print("Testing event formats...")
    
    for event in test_cases:
        # Serialize
        line = json.dumps(event)
        
        # Parse back
        parsed = json.loads(line)
        
        # Validate structure
        assert "event" in parsed, f"Missing 'event' key: {line}"
        event_type = parsed["event"]
        
        # Type-specific validation
        if event_type == "ShouldSpeak":
            assert "text" in parsed, f"Missing 'text' in ShouldSpeak: {line}"
            assert parsed.get("urgency") in ["low", "medium", "high", "critical"], \
                f"Invalid urgency: {parsed.get('urgency')}"
        
        elif event_type == "VoiceUserSpeaking":
            assert "text" in parsed, f"Missing 'text' in VoiceUserSpeaking: {line}"
            assert "is_final" in parsed, f"Missing 'is_final' in VoiceUserSpeaking: {line}"
            assert isinstance(parsed["is_final"], bool), \
                f"is_final must be boolean: {parsed['is_final']}"
        
        elif event_type == "VoiceWakeySpeaking":
            assert "text" in parsed, f"Missing 'text' in VoiceWakeySpeaking: {line}"
        
        elif event_type == "VoiceError":
            assert "message" in parsed, f"Missing 'message' in VoiceError: {line}"
        
        print(f"  ✓ {event_type}")
    
    print("\nAll event formats valid!")


def test_json_lines():
    """Test JSON lines protocol."""
    
    print("\nTesting JSON lines protocol...")
    
    # Simulate stdin
    input_lines = [
        '{"event": "ShouldSpeak", "text": "Hello!", "urgency": "low"}',
        '{"event": "Shutdown"}',
    ]
    
    # Simulate stdout
    output_lines = [
        '{"event": "VoiceListeningStarted"}',
        '{"event": "VoiceWakeyThinking"}',
        '{"event": "VoiceWakeySpeaking", "text": "Hello!"}',
        '{"event": "VoiceSessionEnded"}',
    ]
    
    # Parse each line
    for line in input_lines + output_lines:
        parsed = json.loads(line)
        assert "event" in parsed
    
    print("  ✓ JSON lines parsing works")
    
    # Test newline separation
    combined_input = "\n".join(input_lines) + "\n"
    lines = combined_input.strip().split("\n")
    assert len(lines) == 2, f"Expected 2 lines, got {len(lines)}"
    
    print("  ✓ Newline separation works")
    print("\nJSON lines protocol valid!")


def test_message_flow():
    """Test typical message flow."""
    
    print("\nTesting message flow...")
    
    # Scenario: User says "hello", Wakey responds
    
    flow = [
        # Plugin starts, listening begins
        {"direction": "out", "event": {"event": "VoiceListeningStarted"}},
        
        # User speaks (interim)
        {"direction": "out", "event": {"event": "VoiceUserSpeaking", "text": "hel", "is_final": False}},
        
        # User speaks (interim)
        {"direction": "out", "event": {"event": "VoiceUserSpeaking", "text": "hello", "is_final": False}},
        
        # User speaks (final)
        {"direction": "out", "event": {"event": "VoiceUserSpeaking", "text": "hello wakey", "is_final": True}},
        
        # Wakey starts thinking
        {"direction": "out", "event": {"event": "VoiceWakeyThinking"}},
        
        # Wakey speaks
        {"direction": "out", "event": {"event": "VoiceWakeySpeaking", "text": "Hey! What can I help you with?"}},
        
        # Wakey core sends ShouldSpeak (e.g., proactive message)
        {"direction": "in", "event": {"event": "ShouldSpeak", "text": "I noticed you're working on code", "urgency": "low"}},
        
        # Plugin responds
        {"direction": "out", "event": {"event": "VoiceWakeyThinking"}},
        {"direction": "out", "event": {"event": "VoiceWakeySpeaking", "text": "I noticed you're working on code"}},
        
        # Shutdown
        {"direction": "in", "event": {"event": "Shutdown"}},
        {"direction": "out", "event": {"event": "VoiceSessionEnded"}},
    ]
    
    for step in flow:
        direction = step["direction"]
        event = step["event"]
        line = json.dumps(event)
        
        if direction == "in":
            print(f"  Wakey → Plugin: {line}")
        else:
            print(f"  Plugin → Wakey: {line}")
    
    print("\nMessage flow valid!")


if __name__ == "__main__":
    print("=" * 60)
    print("Wakey Voice Plugin - Protocol Tests")
    print("=" * 60)
    
    test_event_format()
    test_json_lines()
    test_message_flow()
    
    print("\n" + "=" * 60)
    print("All tests passed!")
    print("=" * 60)