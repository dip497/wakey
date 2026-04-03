#!/bin/bash
# Auto-setup for voice-livekit plugin
# Called by Wakey's plugin host on first run

PLUGIN_DIR="$(cd "$(dirname "$0")" && pwd)"
VENV_DIR="$PLUGIN_DIR/.venv"

# Create venv if not exists
if [ ! -d "$VENV_DIR" ]; then
    echo '{"event": "PluginSetup", "message": "Installing voice plugin dependencies..."}'
    uv venv "$VENV_DIR" 2>/dev/null || python3 -m venv "$VENV_DIR"
    "$VENV_DIR/bin/pip" install -r "$PLUGIN_DIR/requirements.txt" -q 2>/dev/null || \
    uv pip install --python "$VENV_DIR/bin/python" -r "$PLUGIN_DIR/requirements.txt" 2>/dev/null
    echo '{"event": "PluginSetup", "message": "Voice plugin ready"}'
fi

# Run the plugin with the venv python
exec "$VENV_DIR/bin/python" "$PLUGIN_DIR/main.py"
