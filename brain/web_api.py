from flask import Flask, jsonify, send_from_directory, request
import state
from commands import send_command
import os

app = Flask(__name__)


@app.after_request
def add_csp_headers(response):
    response.headers["Content-Security-Policy"] = (
        "default-src 'self'; "
        "script-src 'self'; "
        "style-src 'self'; "
        "img-src 'self'; "
        "connect-src 'self';"
    )
    return response


@app.route("/.well-known/appspecific/<path:_>")
def chrome_probe(_):
    return "", 204


@app.route("/")
def root():
    ui_path = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "ui", "frontend")
    )
    return send_from_directory(ui_path, "index.html")


@app.route("/status")
def status():
    if not state.UI_ACTIVE:
        return jsonify({"error": "UI Disconnected"}), 403
    
    if state.LATEST_STATE is None:
        return jsonify({"status": "initializing"})
    
    full_state = state.LATEST_STATE.copy()
    full_state["pending_confirmations"] = state.get_confirmations()
    full_state["security_alerts"] = state.SECURITY_ALERTS
    return jsonify(full_state)


@app.route("/logs")
def logs():
    if not state.UI_ACTIVE:
        return jsonify({"error": "UI Disconnected"}), 403
    return jsonify(state.EXECUTION_LOGS)


# Hardcoded token for development
FLUFFY_TOKEN = "fluffy_dev_token"

@app.route("/command", methods=["POST"])
def command():
    # 1. Restrict to loopback
    if request.remote_addr not in ("127.0.0.1", "::1"):
        return jsonify({"error": "Forbidden - Loopback execution only"}), 403

    # 2. Token guard
    token = request.headers.get("X-Fluffy-Token")
    if token != FLUFFY_TOKEN:
        return jsonify({"error": "Unauthorized - Invalid token"}), 401

    # 3. Validate JSON payload
    cmd_data = request.get_json(silent=True)
    if not cmd_data:
        return jsonify({"error": "Invalid JSON payload"}), 400

    # 4. Handle confirmation removal if applicable
    if "Confirm" in cmd_data:
        state.remove_confirmation(cmd_data["Confirm"]["command_id"])
    elif "Cancel" in cmd_data:
        state.remove_confirmation(cmd_data["Cancel"]["command_id"])

    send_command(cmd_data)
    return jsonify({"ok": True})


@app.route("/security_action", methods=["POST"])
def security_action():
    if request.remote_addr not in ("127.0.0.1", "::1"):
        return jsonify({"error": "Forbidden"}), 403
    
    token = request.headers.get("X-Fluffy-Token")
    if token != FLUFFY_TOKEN:
        return jsonify({"error": "Unauthorized"}), 401
    
    data = request.get_json(silent=True)
    if not data or "pid" not in data or "action" not in data:
        return jsonify({"error": "Invalid payload"}), 400
    
    pid = int(data["pid"])
    action = data["action"]
    
    if state.MONITOR:
        if action == "ignore":
            state.MONITOR.mark_ignored(pid)
            state.add_execution_log(f"Process {pid} ignored by user", "info")
        elif action == "trust":
            state.MONITOR.mark_trusted(pid)
            state.add_execution_log(f"Process {pid} marked as trusted", "info")
            
    return jsonify({"ok": True})


@app.route("/ui_connected", methods=["GET", "POST"])
def ui_connected():
    state.UI_ACTIVE = True
    state.add_execution_log("UI Dashboard connected", "system")
    return jsonify({"status": "UI_CONNECTED", "ui_active": state.UI_ACTIVE})


@app.route("/ui_disconnected", methods=["GET", "POST"])
def ui_disconnected():
    state.UI_ACTIVE = False
    state.add_execution_log("UI Dashboard disconnected", "system")
    return jsonify({"status": "UI_DISCONNECTED", "ui_active": state.UI_ACTIVE})


@app.route("/<path:filename>")
def static_files(filename):
    ui_path = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "ui", "frontend")
    )
    return send_from_directory(ui_path, filename)


def start_api():
    app.run(host="127.0.0.1", port=5123, debug=False, use_reloader=False)
