# Controller Bring-Up Under Brick-Safe Memboot — Findings

> Closes the open loop from
> `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md` (controller was
> the remaining PARTIAL item). **Verdict: SOLVED.** The NES Classic pad now
> emits input under the brick-safe RAM-memboot and **RetroArch RGUI is navigable
> by the controller on HDMI** (user-confirmed — quit RA from the menu with the
> pad). Zero NAND writes; power-cycle = bone stock.

## TL;DR — the working recipe (all RAM-only, brick-safe)

```sh
# 1. modules (vermagic 3.4.113.29-madmonkey, from hakchi-latest.hmod — see gpu-memboot findings)
insmod /tmp/mods/input-polldev.ko
insmod /tmp/mods/mali.ko
insmod /tmp/mods/clovercon.ko module_params=1,195,2,194   # dp-nes: con0 bus1/gpio PG3, con1 bus2/gpio PG2
insmod /tmp/mods/evdev.ko                                  # creates /dev/input/event* (Clovercon = event24)

# 2. THE KEY STEP: run stock clover-mcp ONCE. It opens the pad via SDL, which
#    drives clovercon's setup() to succeed -> "opened controller 1, controller
#    in OK state". clover-mcp then exits (no menu data on memboot); the OK state
#    PERSISTS. After this, event24 emits real button events.
setsid env HOME=/root /usr/bin/clover-mcp </dev/null >/tmp/mcp.log 2>&1 &
sleep 5            # wait for "controller in OK state" in dmesg

# 3. tag the pad so RetroArch's udev joypad driver will accept it
/sbin/udevd -d
udevadm trigger --subsystem-match=input --action=add      # input subsystem ONLY (protects RNDIS/SSH)
udevadm settle --timeout=10
#   -> event24 gains ID_INPUT_JOYSTICK=1, DEVLINKS .../by-path/platform-twi.1-event-joystick

# 4. RetroArch with the udev joypad driver (NOT linuxraw)
#    /tmp/ra-input.cfg:
#        input_joypad_driver = "udev"
#        input_driver        = "udev"
#        menu_driver         = "rgui"
setsid env HOME=/tmp/ra/etc/libretro LD_LIBRARY_PATH=/usr/lib \
  /tmp/ra/bin/retroarch -c /tmp/ra/etc/libretro/retroarch.cfg \
  --appendconfig /tmp/ra-input.cfg -v </dev/null >/tmp/ra.log 2>&1 &
#   -> [udev]: Plugged pad: Nintendo Clovercon - controller1 (0:0) on port #0
#   -> [autoconf]: selected .../joypad_autoconf/clovercon1.cfg  (correct button map)
#   -> RGUI navigable by the pad
```

## Root cause (corrected — the prior handoff lead was wrong in mechanism)

**Symptom:** clovercon loads and `probed controller 1` (pad is the I2C Wii-Classic
"classic" at `twi.1/i2c-1/1-0052` → `/dev/input/event24`), detect GPIO PG3 reads
present (`added device for controller 1`), but `cat /dev/input/event24` while
mashing = **0 bytes**. clovercon never logged `setup succeeded` → the I2C
handshake to 0x52 was failing.

The 2026-06-02 handoff guessed **"clover-mcp = MCU/port power"** and **"twi1 has
no twi_regulator"**. Both ruled out this session:

- **`twi1 has no twi_regulator` is benign** — the same line prints for
  `twi0/twi1/twi2/uart0/uart1/spi-0/spi-1`; it's the stock sunxi kernel's normal
  "no regulator in DT" message. Stock boot prints it identically.
- **Not a PMIC power gate** — `/sys/class/regulator` shows all candidate 3.3 V
  pad rails (`axp22_dldo1/dldo2/aldo1`) already `enabled`. The FEX
  (`/sys/class/script` dump) has **no controller-port power GPIO/regulator**, and
  **no stock init script** (`/newroot/etc/init.d/*`) toggles any port power.
- **Not an MCU-over-UART gate** — `uart2` (the MCU UART clover-mcp opens as
  `/dev/ttyS2`) is `uart_used=0` in the FEX, i.e. disabled in stock too; ttyS2
  never exists. So clover-mcp's ttyS2 open is a no-op here.

**Actual mechanism:** clovercon registers an `input_polldev`; its poll loop (and
the `setup()` I2C handshake inside it) is **gated on the input device being
opened**. Until something opens `event24`, the handshake is never (successfully)
driven to completion. Running stock **`clover-mcp`** opens the pad via SDL's
GameController layer; that open finally lets clovercon's `setup()` complete →
`opened controller 1, controller in OK state`. The OK state then **persists**
after clover-mcp exits (a plain `cat` — clover-mcp already dead — captured 814
real button events). So the "fix" is a one-shot kick from clover-mcp, not any
power/MCU init. (Exact reason a bare `cat` open earlier didn't latch OK, while
clover-mcp's SDL open did, is unconfirmed — likely RA's stale half-open of
event24 interfering before it was killed; clover-mcp ran only after RA was
killed. Not worth re-deriving — the recipe is reliable.)

## RetroArch integration gotcha (second non-obvious key)

Even with the pad emitting, RA saw nothing at first:

- `input_joypad_driver = "linuxraw"` (what the prior session left set) maps
  **joypad-port → `/dev/input/eventN`** (port 0 → event0). The pad is **event24**
  → out of range → never opened. (event0/event1 are `axp22-supplyer` and
  `sunxi-ths`, not joypads.)
- Switching to `input_joypad_driver = "udev"` was necessary but **not
  sufficient**: RA's udev joypad driver filters devices by the
  **`ID_INPUT_JOYSTICK`** property, assigned by udev's `input_id` builtin. With
  no udevd having processed the node, the property is absent → RA skips event24
  (`Couldn't open any... permissions set correctly for /dev/input/event*?`).
- Running **`udevd` + `udevadm trigger --subsystem-match=input`** tags event24
  (`ID_INPUT_JOYSTICK=1`) → RA's udev driver grabs it
  (`Plugged pad: Nintendo Clovercon ... on port #0`) and auto-selects the
  bundled `clovercon1.cfg` button map. **Trigger the `input` subsystem only** —
  a full coldplug risks re-triggering the USB gadget / RNDIS NIC = the SSH
  lifeline; input-only left the network intact.

## Evidence (HW-confirmed, NES Classic `dp-nes`, kernel `3.4.113.29-madmonkey`)

- Before: `event24` raw read = **0 bytes**; dmesg had `added device` + `probed
  controller 1` but **never** `setup succeeded`.
- After `clover-mcp`: dmesg `opened controller 1, controller in OK state`; a
  detached `cat /dev/input/event24` captured **13 024 bytes = 814 input_event
  records** while the user mashed — real `EV_KEY` press/release pairs
  (`code 0x130 BTN_SOUTH=A`, `0x131 BTN_EAST=B`, …) + `EV_SYN` frames.
- After udevd-tag + RA(udev): RA holds `fd → /dev/input/event24`,
  `Plugged pad: Nintendo Clovercon - controller1 (0:0) on port #0`,
  `clovercon1.cfg` autoconfig, Mali GLES2 RGUI @1280×720 — **user navigated the
  RGUI menu and quit RA with the controller.**

## Tooling notes (this session)

- Direct I2C probe of 0x52 was **not** possible: busybox has
  `i2cget/i2cdetect/i2cdump/i2cset`, but `i2c-dev` is absent from the kernel,
  from `hakchi-latest.hmod`, AND from the stock NAND module tree — no
  vermagic-matched `.ko` exists to load. clovercon's `setup succeeded` /
  `OK state` dmesg line was the only available I2C oracle.
- Useful sunxi runtime interfaces present under memboot: `/sys/class/script`
  (`echo <mainkey> > dump; cat dump` → parsed FEX), `/sys/class/sunxi_dump`
  (register read/write — do **not** use `write`), `/sys/class/gpio`
  (export/unexport works). GPIO numbering: chip base 0, so clovercon detect
  `195 = PG3`, `194 = PG2`; twi1 (pad bus) = `PH4/PH5`.

## Open follow-ups (not blockers)

- **Persistence model:** the recipe is manual on-device steps. For the dashboard
  product, these need to become part of the memboot bring-up RetroForge drives
  (insmod set + a one-shot clover-mcp kick + udevd input-tag + RA udev cfg).
- **Minimal kick:** confirm whether a non-clover-mcp opener (with RA killed
  first) also latches OK — would drop the clover-mcp dependency. Low priority;
  clover-mcp is a stock binary and reliable.
