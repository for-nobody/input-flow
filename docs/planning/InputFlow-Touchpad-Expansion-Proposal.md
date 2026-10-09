# InputFlow Touchpad Expansion Proposal

> Status: Future Feature Proposal  
> Purpose: Preserve and formalise the current product ideas for future InputFlow development.  
> Scope: Windows Precision Touchpad enhancement, touch-based click semantics, presentation interaction, virtual numpad, and general touchpad layers.  
> Important: This document is a **future roadmap / design proposal**, not a requirement for the first public release.

---

## 1. Background

InputFlow originally focuses on remapping and reinterpreting Windows input events, including keyboard and mouse input.

During product discussion, several related ideas emerged around the Windows Precision Touchpad.

The common idea behind all of them is:

> InputFlow should not only remap existing shortcuts.  
> It should be able to reinterpret physical input and provide a more natural interaction layer on top of Windows.

Instead of treating the touchpad only as a conventional mouse replacement, InputFlow could eventually treat it as a programmable touch surface.

This document records the current ideas before implementation begins.

---

# 2. Product Direction

The long-term concept can be described as:

> **InputFlow changes how Windows interprets physical input.**

The future touchpad subsystem could provide three main categories of behaviour:

1. **Touch semantics**
   - Short touch
   - Long touch
   - Tap / double tap
   - Movement cancellation
   - Custom gesture recognition

2. **Touchpad layers**
   - Numpad
   - Media pad
   - Presentation pad
   - Custom region layouts

3. **Pointer modes**
   - Normal cursor
   - Presentation pointer
   - Virtual laser pointer
   - Relative movement
   - Absolute touchpad-to-screen mapping

These features should eventually share a common Precision Touchpad input engine instead of being implemented as unrelated hacks.

---

# 3. Feature A — Touch Click

## 3.1 Goal

Replace the traditional Windows touchpad click model with a more modern touch-based interaction.

The user should **not need to physically press the touchpad down**.

Desired default behaviour:

```text
Short touch  -> Left Click
Long touch   -> Right Click
Movement     -> Cursor movement only
```

This is conceptually closer to modern phone touch interaction than to a mechanical clickpad.

---

## 3.2 Intended Interaction

Example:

```text
Finger touches touchpad
        |
        +-- released quickly ----------------> Left Click
        |
        +-- held nearly still long enough ---> Right Click
        |
        +-- moved beyond threshold ----------> Cancel click recognition
                                               Continue normal pointer movement
```

Possible initial timing:

```text
Short touch:
    release before approximately 350-400 ms

Long touch:
    remain touching for approximately 400-500 ms
```

The exact values must be configurable and validated through real use.

---

## 3.3 Required Gesture State Machine

A simple implementation should not be:

```text
if touch_duration > threshold:
    right_click()
```

That would produce false right-clicks whenever the user pauses while moving the pointer.

Instead, use an explicit state machine.

Example:

```text
IDLE
 |
 | new single-finger contact
 v
TOUCH_PENDING
 |
 +-- movement > movement_threshold --> POINTER_MOVEMENT
 |
 +-- release before hold threshold --> LEFT_CLICK
 |
 +-- held still past threshold -----> RIGHT_CLICK
```

Recommended state information:

```text
contact_id
touch_start_time
initial_x
initial_y
current_x
current_y
distance_from_origin
gesture_state
action_consumed
```

---

## 3.4 Movement Cancellation

The gesture must distinguish between:

- deliberate tap
- deliberate hold
- pointer movement

Suggested rule:

```text
if movement_distance > threshold:
    cancel tap/hold recognition
```

The movement threshold should use physical or normalised touchpad distance where possible rather than raw arbitrary units.

The correct threshold should be determined by hardware testing.

---

## 3.5 Windows Native Tap Conflict

Windows Precision Touchpad already supports native single-finger tap-to-click.

If InputFlow also injects a click after recognising a short touch, the result could become:

```text
Windows native tap -> Left Click
InputFlow tap       -> Left Click

Result:
Double Click
```

Therefore the design must account for Windows native tap behaviour.

Possible strategy:

```text
Disable Windows native single-finger tap
InputFlow becomes responsible for Touch Click semantics
```

However:

- InputFlow should never silently modify permanent Windows settings.
- Any Windows setting changes must be explicit, reversible, and clearly explained.
- Original values should be preserved.
- Unexpected shutdown should not leave the user with unusable touchpad behaviour.

---

## 3.6 Physical Click Behaviour

Physical click behaviour should initially remain separate.

Possible configuration:

```text
Touch:
    short -> left
    long  -> right

Physical click:
    keep Windows default
```

Future versions may allow physical click remapping, but this should not be required for the initial Touch Click prototype.

---

# 4. Feature B — Presentation Pointer / Virtual Laser

## 4.1 Goal

Provide a system-wide presentation pointer that works outside PowerPoint.

The user should be able to enter a temporary presentation mode using an InputFlow trigger, then use the touchpad as a virtual laser pointer.

Example use cases:

- PowerPoint
- Google Slides
- PDF readers
- browsers
- Teams
- Zoom
- code demonstrations
- image viewers
- desktop demonstrations

The feature should not depend on presentation software-specific APIs.

---

## 4.2 Activation

One proposed activation method:

```text
Ctrl Ctrl Ctrl
```

That means:

```text
Ctrl Down
Ctrl Up
short interval
Ctrl Down
Ctrl Up
short interval
Ctrl Down
Ctrl Up
-> Presentation Mode ON
```

The recogniser must reject normal shortcuts.

For example:

```text
Ctrl + C
Ctrl + V
Ctrl + S
```

must never accidentally activate presentation mode.

Recommended rule:

```text
Repeated modifier activation is valid only when:
- the modifier is pressed and released independently
- no other key is pressed between taps
- all taps occur within the configured timing window
```

Activation should eventually be configurable.

Possible options:

```text
Ctrl x2
Ctrl x3
Shift x3
Caps Lock x2
Custom chord
```

---

## 4.3 Visual Behaviour

When Presentation Mode activates:

```text
Normal cursor:
    hidden, faded, or visually de-emphasised

Virtual pointer:
    rendered as a laser-like dot or ring
```

Possible pointer styles:

```text
Laser dot
Ring
Spotlight
Crosshair
Custom size
Custom opacity
```

The visual overlay must remain lightweight and low-latency.

---

## 4.4 Relative Mode

Relative mode behaves like a normal mouse:

```text
Finger moves 20 mm right
-> laser pointer moves relative to current screen position
```

Advantages:

- familiar
- easy to implement
- compatible with standard pointer behaviour

---

## 4.5 Absolute Mode

Absolute mode maps the touchpad surface directly to the display.

Example:

```text
Touchpad top-left    -> Screen top-left
Touchpad centre      -> Screen centre
Touchpad bottom-right-> Screen bottom-right
```

Concept:

```text
Touchpad coordinates:
(x_touch, y_touch)

normalised:
nx = x_touch / touchpad_width
ny = y_touch / touchpad_height

screen:
x_screen = nx * screen_width
y_screen = ny * screen_height
```

This would turn the touchpad into a small presentation pad or tablet-like pointing surface.

It may be particularly useful during presentations because the presenter can jump directly to a screen region.

---

## 4.6 Presentation Controls

Future presentation layer actions could include:

```text
Short touch        -> optional click / no action
Two-finger tap     -> Next Slide
Three-finger tap   -> Previous Slide
Long touch         -> Highlight
Edge region        -> temporary menu
Esc                -> Exit Presentation Mode
Activation gesture -> toggle mode
```

The first version should remain minimal.

---

# 5. Feature C — Touchpad Numpad

## 5.1 Goal

Allow a normal Windows Precision Touchpad to act as a virtual numeric keypad, without requiring special illuminated touchpad hardware.

This concept is inspired by laptop designs where the touchpad can also become a numpad, but InputFlow should provide a software-only implementation.

---

## 5.2 Initial Layout

A simple 3 x 4 layout is recommended:

```text
+-------------------+
|   7     8     9   |
|                   |
|   4     5     6   |
|                   |
|   1     2     3   |
|                   |
|         0         |
+-------------------+
```

This should be the first prototype because:

- only 10 digits are required
- each touch region can remain large
- blind use may become practical on sufficiently large touchpads
- complexity remains low

A full 17-key numpad should not be the first implementation.

---

## 5.3 Absolute Region Mapping

The numpad should use absolute touchpad coordinates.

Example:

```text
Touchpad logical bounds:
X = 0 ... X_MAX
Y = 0 ... Y_MAX
```

The touchpad can then be divided into logical regions.

Basic mapping:

```text
row    = normalised_y -> layout row
column = normalised_x -> layout column
```

The system should never depend on screen cursor position.

---

## 5.4 Dead Zones / Gutters

Do not allow adjacent regions to touch directly.

Bad:

```text
+----+----+----+
|  7 |  8 |  9 |
+----+----+----+
```

Better logical design:

```text
+----+  +----+  +----+
|  7 |  |  8 |  |  9 |
+----+  +----+  +----+
```

The unused gaps act as dead zones.

Reason:

```text
Touch near boundary
-> Ignore input

rather than

Touch near boundary
-> Guess wrong number
```

For a virtual keypad without physical key borders, false input is worse than requiring a second tap.

Possible starting point:

```text
5-10% gutter between neighbouring regions
```

This must be validated experimentally.

---

## 5.5 On-Screen Guide

Because normal touchpads do not illuminate key boundaries, InputFlow should optionally show an on-screen guide.

Example:

```text
+------------------+
| Touch Numpad     |
|                  |
|   7   8   9      |
|   4   5   6      |
|   1   2   3      |
|       0          |
+------------------+
```

Possible settings:

```text
Guide:
- Always
- Show for 2 seconds
- Show until first input
- Never
```

New users can use the overlay while learning the physical layout.

Experienced users may eventually use the touchpad by spatial memory.

---

## 5.6 Blind Use

The touchpad already provides physical reference points:

- left edge
- right edge
- top edge
- bottom edge
- four corners
- centre

For a sufficiently large touchpad, a 3 x 4 digit grid may become usable through muscle memory.

This should be treated as a hypothesis to test, not an assumption.

Suggested experiment:

```text
Test sequence:
1234567890

Repeat:
20-50 times

Measure:
- error rate
- average input time
- boundary mis-taps
- adaptation after repeated use
```

---

# 6. Feature D — General Touchpad Layers

The Numpad should eventually be implemented as one preset of a more general **Touchpad Layer / Region Engine**.

Core concept:

```text
Touchpad Region
      |
      v
InputFlow Action
```

The same engine can support multiple layouts.

---

## 6.1 Numpad Layer

```text
7 8 9
4 5 6
1 2 3
  0
```

---

## 6.2 Calculator Layer

Possible example:

```text
7 8 9 /
4 5 6 *
1 2 3 -
0 . = +
```

---

## 6.3 Media Layer

Example:

```text
+---------------------+
| Prev   Play   Next  |
| Vol-   Mute   Vol+  |
| Back   Home   Fwd   |
+---------------------+
```

---

## 6.4 Presentation Layer

Example:

```text
+---------------------+
| Prev        Laser   |
|                     |
| Blank       Next    |
+---------------------+
```

---

## 6.5 Developer / IDE Layer

Example:

```text
+---------------------+
| Build   Run   Debug |
| Git     Term  Test  |
| Undo    Redo  Save  |
+---------------------+
```

---

## 6.6 Custom Layer

Future users should be able to create arbitrary regions and map them to existing InputFlow actions.

Possible actions:

- keyboard shortcut
- mouse button
- launch application
- open file
- open folder
- open URL
- media action
- InputFlow command
- switch layer
- enable / disable mode

---

# 7. Shared Technical Foundation

These features should eventually share one subsystem.

Recommended high-level structure:

```text
Windows Precision Touchpad
            |
            v
      Raw Input / HID
            |
            v
      Contact Tracking
            |
      +-----+------+------+
      |            |      |
      v            v      v
 Gesture Engine  Region  Pointer
                Engine   Engine
      |            |      |
      v            v      v
 Tap / Hold      Numpad  Laser
 Gestures        Layers  Pointer
            \      |      /
             \     |     /
              v    v    v
            InputFlow Actions
```

---

# 8. Precision Touchpad Input Requirements

The touchpad engine is expected to require access to information such as:

```text
contact_id
tip/contact state
absolute X
absolute Y
contact count
timestamp
```

Optional future data:

```text
pressure
width / height
confidence
additional HID usages
```

Pressure is **not required** for the main Touch Click concept.

The initial concept is based on:

```text
contact + duration + movement
```

not pressure.

---

# 9. Suggested Internal Modules

Possible future module boundaries:

```text
touchpad/
    device.rs
    raw_input.rs
    hid_parser.rs
    contacts.rs
    gesture_engine.rs
    region_engine.rs
    pointer_mode.rs
    settings.rs
```

Conceptual responsibility:

### device.rs
Discover and identify compatible Precision Touchpad devices.

### raw_input.rs
Register for and receive raw touchpad input.

### hid_parser.rs
Decode relevant HID reports.

### contacts.rs
Track active contact IDs and their state.

### gesture_engine.rs
Recognise:

- short touch
- long touch
- double touch
- movement cancellation
- future gestures

### region_engine.rs
Map absolute touch coordinates to configurable regions.

### pointer_mode.rs
Implement:

- normal
- presentation relative
- presentation absolute

### settings.rs
Store touchpad-specific configuration.

These names are illustrative only. Final architecture should follow the existing InputFlow codebase structure.

---

# 10. Interaction Safety Principles

Touchpad features can easily make the computer difficult to control.

Therefore the subsystem must follow strict safety principles.

## 10.1 Always Preserve an Escape Path

Every special touchpad mode must have at least one reliable exit mechanism.

Examples:

```text
Esc
Configured keyboard chord
Tray menu
Timeout
Disable InputFlow
```

---

## 10.2 Fail Open Where Possible

If InputFlow crashes or is disabled:

- normal Windows mouse movement should remain available
- physical click should ideally remain available
- permanent system configuration should not be left in an unexpected state

---

## 10.3 Reversible Windows Settings

If the software temporarily changes Windows touchpad configuration:

```text
Read current value
Store original value
Apply temporary value
Restore original value on exit
```

The program must not assume a default value.

---

## 10.4 Prevent Injected Input Loops

If InputFlow injects:

```text
Left Click
Right Click
Keyboard key
```

the injected events must not be reinterpreted as new physical input and recursively processed.

This is particularly important because InputFlow already performs input remapping.

---

# 11. UX Settings Proposal

Possible future settings page:

```text
Touchpad
|
+-- Touch Click
|   |
|   +-- Enable Touch Click
|   +-- Short Touch -> Left Click
|   +-- Long Touch -> Right Click
|   +-- Hold Duration
|   +-- Movement Threshold
|
+-- Presentation Mode
|   |
|   +-- Activation Gesture
|   +-- Pointer Style
|   +-- Relative / Absolute
|   +-- Exit Gesture
|
+-- Touchpad Layers
    |
    +-- Numpad
    +-- Media
    +-- Presentation
    +-- Custom
```

Advanced settings should remain hidden from ordinary users where possible.

---

# 12. Product Positioning

The proposed touchpad subsystem should not become:

> another giant input-remapping configuration framework

The distinguishing idea is:

> **Make the Windows touchpad behave like a modern programmable touch surface.**

Possible future product language:

> InputFlow reinterprets keyboard, mouse, and touchpad input to create more natural Windows interactions.

Touchpad-specific positioning:

> Turn any compatible Windows Precision Touchpad into a programmable touch surface.

Examples:

```text
Tap           -> Left Click
Hold          -> Right Click
Touch regions -> Numpad / shortcuts
Touchpad      -> Presentation laser pointer
```

---

# 13. Competitive / Product Gap Observation

The underlying techniques are not completely new.

Existing products and projects demonstrate parts of the required technology:

- OEM touchpad numpads
- Precision Touchpad gesture tools
- raw touchpad HID readers
- gesture remappers
- absolute touchpad pointing projects
- presentation pointer utilities

Therefore InputFlow should **not** claim that the underlying technology has never existed.

The possible product opportunity is instead:

> There appears to be no single lightweight, cross-vendor, actively maintained Windows utility whose primary goal is to modernise the basic Precision Touchpad interaction model and expose it as a clean programmable input surface.

This statement should be treated as a product hypothesis, not a marketing fact, until further market research is completed.

---

# 14. Development Priority

These features should **NOT** block the first InputFlow release.

Current priority remains:

```text
Existing InputFlow core
-> finish current Phase F
-> complete current release requirements
-> first stable release
```

The touchpad subsystem should begin only after the current keyboard/mouse core reaches a sufficiently stable state.

Reason:

- Precision Touchpad introduces a new device class
- new raw input parsing
- new gesture state machines
- new UI configuration
- new hardware compatibility testing
- potentially new Windows settings interaction

Adding it immediately would greatly expand the current project scope.

---

# 15. Recommended Future Development Sequence

## Stage 0 — Documentation Only

Current stage.

Goals:

- preserve ideas
- avoid scope creep
- perform no production integration

---

## Stage 1 — Raw Touchpad Feasibility Prototype

Create an isolated experimental program.

It should only:

```text
Detect compatible Precision Touchpad
Read contact events
Print:
- contact ID
- touch down/up
- X
- Y
- duration
```

No InputFlow integration.

No mouse injection.

No Windows settings modification.

Acceptance:

```text
Touch down/up reliably detected
Absolute coordinates stable
Single-finger movement reliably tracked
No impact on normal Windows touchpad operation
```

---

## Stage 2 — Touch Click Experimental Prototype

Implement:

```text
short touch -> left click
long touch  -> right click
movement    -> cancel click
```

Keep this isolated from the main InputFlow core until behaviour is validated.

Test:

- accidental right-click rate
- pointer pause behaviour
- click latency
- double-click behaviour
- drag interaction
- multi-finger interference
- injected event loops

---

## Stage 3 — Integrate Touch Click into InputFlow

Only after the isolated prototype is stable.

Requirements:

- configuration
- enable/disable switch
- safe fallback
- state restoration
- logging / diagnostics

---

## Stage 4 — Presentation Pointer

Implement:

```text
activation gesture
virtual laser overlay
relative pointer mode
safe exit
```

Then add absolute mode.

---

## Stage 5 — Touchpad Numpad

Implement simple digit-only layout:

```text
7 8 9
4 5 6
1 2 3
  0
```

Add:

- dead zones
- visual guide
- basic statistics / debugging
- configuration

---

## Stage 6 — General Touchpad Layers

Extract the Numpad implementation into a reusable region engine.

Add:

- Media preset
- Presentation preset
- Custom preset
- user-defined layouts

---

## Stage 7 — Advanced Gestures

Only after the basic system is mature.

Possible future work:

- multi-finger tap
- swipe
- pinch
- rotate
- edge regions
- multi-stage gestures
- per-app profiles
- gesture chaining

---

# 16. Testing Requirements

Touchpad features must be tested across multiple hardware configurations.

Important variables:

```text
Touchpad size
Touchpad aspect ratio
Laptop manufacturer
Precision Touchpad implementation
Physical clickpad vs haptic touchpad
Screen DPI
Multi-monitor setup
Refresh rate
Windows version
```

Testing should include:

### Touch Click
- short taps
- long holds
- slow pointer movement
- fast pointer movement
- micro-movements while holding
- double tap
- drag attempts
- multi-finger interference

### Presentation Mode
- relative movement
- absolute mapping
- multi-monitor behaviour
- DPI scaling
- full-screen applications
- PowerPoint
- browser slides
- PDF viewer
- Teams / Zoom

### Numpad
- accuracy
- speed
- blind input
- edge taps
- boundary taps
- small touchpads
- large touchpads

---

# 17. Non-Goals for the First Touchpad Prototype

Do not attempt all of the following at once:

```text
Custom Precision Touchpad driver
Kernel filter driver
Full replacement of Windows gesture stack
Pressure-sensitive 3D Touch
All multi-finger gestures
Full custom layout editor
Haptic feedback support
Per-application touchpad profiles
Cloud profile sync
```

The first prototype should prove only:

> Can InputFlow reliably observe Precision Touchpad contact events and distinguish tap, hold, and movement without damaging normal pointer behaviour?

---

# 18. Key Product Experiments

Before committing to a large implementation, validate the following questions.

## Experiment A — Touch Click

Question:

> Does short-touch-left / long-touch-right actually feel better than normal Windows touchpad behaviour?

Measure:

- accidental activation
- perceived latency
- comfort
- adaptation time
- drag usability

---

## Experiment B — Touchpad Numpad

Question:

> Can users reliably enter digits on a non-illuminated touchpad?

Measure:

- error rate
- speed
- learning curve
- dependence on visual guide
- effect of touchpad size

---

## Experiment C — Presentation Pointer

Question:

> Is a touchpad-based virtual laser pointer useful enough to become a recurring presentation workflow?

Compare:

```text
Relative mode
vs
Absolute mode
```

Measure:

- target acquisition
- comfort
- precision
- presentation workflow usefulness

---

# 19. Long-Term Vision

If the experiments succeed, the InputFlow input model could evolve from:

```text
Keyboard
Mouse
```

to:

```text
Keyboard
Mouse
Precision Touchpad
```

and eventually:

```text
Physical Input
      |
      v
Input Interpretation
      |
      v
Gesture / Rule / Region Engine
      |
      v
Action
```

This keeps the original InputFlow philosophy while expanding the kinds of physical input it can reinterpret.

---

# 20. Current Decision

For now:

```text
DO:
- preserve this proposal
- finish the current InputFlow release
- stabilise the existing core
- study the current Rust / Win32 implementation
- revisit touchpad work after the current release

DO NOT:
- add the full touchpad subsystem to the current release
- change current stable input behaviour for experimental features
- begin with a kernel driver
- implement multiple touchpad features simultaneously
```

The first future task should be a **standalone Precision Touchpad Raw Input feasibility prototype**.

That prototype should remain separate from the production InputFlow code until the required touch data can be read reliably and safely.

---

# 21. Summary

The current touchpad ideas are:

```text
1. Touch Click
   Short touch -> Left Click
   Long touch  -> Right Click
   No physical press required

2. Presentation Pointer
   Repeated Ctrl or configurable trigger
   -> enter presentation mode
   -> touchpad controls a virtual laser pointer
   -> relative and absolute modes

3. Touchpad Numpad
   Touchpad divided into large digit regions
   -> software-only numpad
   -> dead zones
   -> optional visual guide
   -> potential blind use

4. Touchpad Layers
   Generalise region mapping
   -> Numpad
   -> Media
   -> Presentation
   -> Developer
   -> Custom

5. Shared Touchpad Engine
   Precision Touchpad Raw Input
   -> contact tracking
   -> gesture engine
   -> region engine
   -> pointer modes
   -> InputFlow actions
```

The most important principle is:

> Build the touchpad subsystem as a coherent input engine, not as a collection of unrelated special cases.

