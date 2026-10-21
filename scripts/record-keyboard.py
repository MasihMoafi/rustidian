"""Observe only Save/modifier X11 events in a private test display."""
import argparse
import json
from pathlib import Path
import signal

from Xlib import X, XK, display
from Xlib.ext import record
from Xlib.protocol import rq


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ready", type=Path, required=True)
    args = parser.parse_args()
    control = display.Display()
    observer = display.Display()
    assert observer.has_extension("RECORD"), "The private X server needs RECORD"
    context = observer.record_create_context(0, [record.AllClients], [{
        "core_requests": (0, 0), "core_replies": (0, 0),
        "ext_requests": (0, 0, 0, 0), "ext_replies": (0, 0, 0, 0),
        "delivered_events": (0, 0), "device_events": (X.KeyPress, X.KeyRelease),
        "errors": (0, 0), "client_started": False, "client_died": False,
    }])
    names = {XK.XK_s: "S", XK.XK_S: "S", XK.XK_Control_L: "Control_L",
             XK.XK_Control_R: "Control_R", XK.XK_Shift_L: "Shift_L", XK.XK_Shift_R: "Shift_R"}
    with args.output.open("w", buffering=1) as output:
        def receive(reply):
            if reply.category == record.StartOfData:
                args.ready.touch()
            if reply.category != record.FromServer or reply.client_swapped:
                return
            data = reply.data
            while data:
                event, data = rq.EventField(None).parse_binary_value(data, observer.display, None, None)
                if event.type not in (X.KeyPress, X.KeyRelease):
                    continue
                name = names.get(control.keycode_to_keysym(event.detail, 0))
                if name:
                    output.write(json.dumps({"key": name, "pressed": event.type == X.KeyPress,
                        "keycode": event.detail, "state": event.state, "time": event.time})+"\n")

        def stop(*_):
            control.record_disable_context(context)
            control.flush()

        signal.signal(signal.SIGTERM, stop)
        try:
            observer.record_enable_context(context, receive)
        finally:
            observer.record_free_context(context)
            observer.close()
            control.close()


if __name__ == "__main__":
    main()
