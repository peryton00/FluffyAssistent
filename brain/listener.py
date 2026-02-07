from state import update_state, add_execution_log
from web_api import start_api
from threading import Thread
from interpreter import interpret
from recommender import recommend
from security_monitor import SecurityMonitor
import state

import socket
import json
import sys
import signal
import time
import copy

IPC_HOST = "127.0.0.1"
IPC_PORT = 9001

shutting_down = False
ipc_socket = None


# -----------------------------
# IPC CONNECTION
# -----------------------------
def connect_ipc():
    while True:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect((IPC_HOST, IPC_PORT))
            s.settimeout(1.0)
            return s
        except (ConnectionRefusedError, TimeoutError):
            time.sleep(0.5)


# -----------------------------
# SHUTDOWN HANDLING
# -----------------------------
def shutdown(signum=None, frame=None):
    global shutting_down, ipc_socket

    if shutting_down:
        return
    shutting_down = True

    print("\n[Fluffy Brain] Shutting down...", file=sys.stderr)

    if ipc_socket:
        try:
            ipc_socket.close()
        except Exception:
            pass


# -----------------------------
# SIGNAL COMPUTATION (AUTHORITATIVE)
# -----------------------------
def compute_signals(msg):
    system = msg.get("system", {})
    ram = system.get("ram", {})
    cpu = system.get("cpu", {})
    processes = system.get("processes", {}).get("top_ram", [])

    signals = {}

    # Memory pressure
    if ram and ram.get("total_mb", 0) > 0:
        usage = (ram["used_mb"] / ram["total_mb"]) * 100
        if usage < 60:
            signals["memory_pressure"] = "LOW"
        elif usage < 75:
            signals["memory_pressure"] = "MEDIUM"
        elif usage < 90:
            signals["memory_pressure"] = "HIGH"
        else:
            signals["memory_pressure"] = "CRITICAL"

    # CPU pressure
    if cpu:
        u = cpu.get("usage_percent", 0)
        if u < 40:
            signals["cpu_pressure"] = "NORMAL"
        elif u < 70:
            signals["cpu_pressure"] = "BUSY"
        else:
            signals["cpu_pressure"] = "OVERLOADED"

    # Offenders (NO SORTING / NO FILTERING)
    if processes:
        signals["top_ram_offender"] = max(processes, key=lambda p: p.get("ram_mb", 0))
        signals["top_cpu_offender"] = max(
            processes, key=lambda p: p.get("cpu_percent", 0)
        )

    return signals


# -----------------------------
# MESSAGE HANDLER
# -----------------------------
def handle_message(raw_msg, monitor):
    # If UI is not active, skip heavy processing and state updates
    if not state.UI_ACTIVE:
        # Still run security monitor in background!
        security_alerts = monitor.analyze(raw_msg, state.UI_ACTIVE)
        state.update_security_alerts(security_alerts)
        return

    # Work on a clean copy to avoid race conditions
    msg = copy.deepcopy(raw_msg)

    signals = compute_signals(msg)
    msg["signals"] = signals

    # Security Analysis
    security_alerts = monitor.analyze(msg, state.UI_ACTIVE)
    state.update_security_alerts(security_alerts)

    interpretations = interpret(msg)
    recommendations = recommend(msg)

    msg["_insights"] = interpretations
    msg["_recommendations"] = recommendations

    add_execution_log("Telemetry received from core", "system")

    # Push ONE complete snapshot to UI
    update_state(msg)

    if interpretations:
        print("\n[Fluffy Insight]")
        for line in interpretations:
            print(f"* {line}")

    if recommendations:
        print("\n[Fluffy Suggestion]")
        for rec in recommendations:
            print(f"-> {rec}")


# -----------------------------
# MAIN LOOP
# -----------------------------
def main():
    global ipc_socket

    Thread(target=start_api, daemon=True).start()

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    monitor = SecurityMonitor()
    state.MONITOR = monitor

    ipc_socket = connect_ipc()
    print("[Fluffy Brain] Connected to IPC", file=sys.stderr)

    buffer = ""

    try:
        while not shutting_down:
            try:
                try:
                    data = ipc_socket.recv(4096)
                except socket.timeout:
                    continue
                except ConnectionResetError:
                    add_execution_log("Core disconnected", "error")
                    break
                except OSError:
                    break

                if not data:
                    add_execution_log("IPC disconnected. Reconnecting...", "error")
                    ipc_socket.close()
                    ipc_socket = connect_ipc()
                    continue

                buffer += data.decode()

                while "\n" in buffer:
                    line, buffer = buffer.split("\n", 1)

                    if not line.strip():
                        continue

                    try:
                        raw = json.loads(line)
                        
                        # Support for both wrapped and unwrapped messages
                        msg_data = raw.get("payload", raw) if isinstance(raw, dict) else raw

                        # --- CONFIRMATION REQUEST FROM RUST ---
                        if isinstance(msg_data, dict) and msg_data.get("type") == "confirm_required":
                            cmd_id = msg_data.get("command_id")
                            cmd_name = msg_data.get("command_name", "Unknown Command")
                            details = msg_data.get("details", "")
                            
                            from state import add_confirmation
                            add_confirmation(cmd_id, cmd_name, details)
                            
                            add_execution_log(
                                f"Confirmation required for {cmd_name} (id={cmd_id})",
                                "action",
                            )
                            continue

                        # --- EXECUTION RESULT FROM RUST ---
                        if isinstance(msg_data, dict) and msg_data.get("type") == "execution_result":
                             add_execution_log(
                                f"Command {msg_data.get('command')} {msg_data.get('status')}",
                                "info"
                            )
                             continue

                        handle_message(msg_data, monitor)

                    except json.JSONDecodeError as e:
                        print(f"[JSON ERROR] {e}", file=sys.stderr)
            except Exception as e:
                print(f"[ERROR] Error in main loop: {e}", file=sys.stderr)
                add_execution_log(f"Error in main loop: {e}", "error")
                break
    finally:
        shutdown()


if __name__ == "__main__":
    main()
