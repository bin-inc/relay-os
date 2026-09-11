# NUC Milestone One Acceptance

## Target

- Intel NUC 12 Pro `RNUC12WSHI70000`, Core i7-1260P.
- One directly connected supported USB keyboard and one flashed Relay OS USB drive.
- Secure Boot disabled before booting the image.

## Task 5 Runtime Gate Procedure

1. Build `target/relay-os.img` with `cargo xtask image --output target/relay-os.img`.
2. Record the USB device identity and SHA-256 of the exact flashed image.
3. Flash that image using an explicit host disk-imaging tool and safely eject it.
4. Enter NUC firmware setup, disable Secure Boot, and record the firmware version and changed setting.
5. Boot the USB drive with no serial terminal attached.
6. Photograph the visible framebuffer banner containing both lines below, preserving enough context to identify the NUC and boot media:

   ```text
   [relay] phase=kernel-entry status=ok
   [relay] phase=kernel-runtime status=ok
   ```

7. Save the photo hash, firmware version, image hash, date, and operator in the evidence table.
8. Do not begin Task 11 xHCI work until this physical gate is recorded as passed.

## Physical Acceptance

The clean production image completed the Task 5 NUC runtime gate. The captured
banner shows both required kernel status lines. The source photo is retained in
the separate `relay-os-artifacts` repository so this source repository contains
text evidence only.

## Evidence

| Field | Value |
| --- | --- |
| Status | Passed: clean production image displayed both required kernel status lines on the target NUC. |
| Firmware version | `WSADL357.0090.2023.0821.1714` |
| Secure Boot state | Disabled |
| USB device identity | Kingston DataTraveler 3.0, 14.40 GB, serial `0B82670162E7` |
| Flashed image SHA-256 | `b4c876089aba89510e484412fab75c94ceba072c9fc24816cf4d2b6b30b1418d` |
| Banner photo and SHA-256 | `relay-os-artifacts/evidence/20260911-020152-aade158c-20260911_085103.jpg` at artifact commit `2eb630a`; SHA-256 `5a9af08e6affb907bdab7da210723f42ca731649e096cb879bf6bcf5744b32c6` |
| Operator and date | `maw629`, 2026-09-11 |

The banner photo visibly contains both required Task 5 lines:

```text
[relay] phase=kernel-entry status=ok
[relay] phase=kernel-runtime status=ok
```
