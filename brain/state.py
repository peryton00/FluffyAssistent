from threading import Lock

LATEST_STATE = None
EXECUTION_LOGS = []
PENDING_CONFIRMATIONS = []
SECURITY_ALERTS = []
MONITOR = None
UI_ACTIVE = False
LOCK = Lock()


def update_state(state_update):
    global LATEST_STATE
    with LOCK:
        if LATEST_STATE is None:
            LATEST_STATE = state_update
        else:
            LATEST_STATE.update(state_update)


def add_execution_log(message, level="info"):
    with LOCK:
        EXECUTION_LOGS.append({"message": message, "level": level})


def add_confirmation(cmd_id, cmd_name, details):
    with LOCK:
        PENDING_CONFIRMATIONS.append({
            "command_id": cmd_id,
            "command_name": cmd_name,
            "details": details
        })


def get_confirmations():
    with LOCK:
        return list(PENDING_CONFIRMATIONS)


def remove_confirmation(cmd_id):
    global PENDING_CONFIRMATIONS
    with LOCK:
        PENDING_CONFIRMATIONS = [c for c in PENDING_CONFIRMATIONS if c["command_id"] != cmd_id]


def update_security_alerts(alerts):
    global SECURITY_ALERTS
    with LOCK:
        SECURITY_ALERTS = alerts
