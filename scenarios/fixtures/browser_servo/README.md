# Upstream Servo fixture

Serve this directory with `python -m http.server 43124 --bind 127.0.0.1
--directory scenarios/fixtures/browser_servo`, then set `SERVO_FIXTURE_BASE`
to `http://127.0.0.1:43124` for `browser_servo_windows.scn`.

Run a fresh Turnstone profile with the optional `servo` feature, the explicitly
named `TURNSTONE_SERVO_PROFILE`, and a dedicated `TURNSTONE_SERVO_PROFILE_DIR`.
Both views use that process profile. The fixture makes no cookie, localStorage,
or persistence claim.

The native page script changes the real document title as input arrives and
navigates to a URL containing the actual input when its button receives a click.
Assertions read normal title/address callbacks. They do not request host script
evaluation. Red TOP and blue BOTTOM strips make GL orientation visible before
and after resize, navigation and reconstruction. Captures must be reviewed;
frame counters alone cannot prove correct pixels or orientation.

`browser_servo_intl_windows.scn` uses the separate `intl.html` fixture to check
native-page normalization, calendar date formatting, word segmentation and
numeric collation through SpiderMonkey's ICU boundary. The title reports the
combined result, and the capture shows each individual result. It does not use
host script evaluation or change the existing input/orientation fixture.
