from flask import Flask, jsonify
import state

app = Flask(__name__)

@app.route("/test", methods=["POST"])
def test():
    return jsonify({"ok": True})

if __name__ == "__main__":
    app.run(port=5555)
