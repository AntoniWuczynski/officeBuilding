# DESIGN.md — Office Building
Date: 2026-09-29   Status: diverged
Codified from: new (the previous scaffold's dark oklch theme was an agent default, never an owner decision, and is replaced)
Identity: The only agent manager where your coding agents work in a cutaway office building you can see into, and every manager's panel is an object on the floor.
Volume default: canvas-first. The building and floor scenes are the content and carry the identity. Chrome around them (directory bar, rail, sidebar, terminal) is quiet signage: paper plates, ink borders, no shadows.
Tokens:
  --sky-top: #CFE3EE; --sky-bottom: #EEF4F1 (the one gradient: the sky behind the scene)
  --plaster: #F1ECE2; --plaster-shade: #E2DACB (walls)
  --boards: #D9BD93 (floors); --concrete: #BDB6A8; --concrete-shade: #A39C8E (slabs)
  --timber: #9B7753; --timber-front: #7A5A3C; --timber-side: #634831 (desks)
  --glass: rgba(200,228,240,.36) (manager's office)
  --ink: #1C1F24 (text, sign borders); --paper: #FFFDF8 (signs, panels); --rule: #CFC7B6
  --signage: #B5532B (enamel sign red: roof sign, primary action only)
  Status, semantic only: --ok #2FBF71 (healthy), --wait #F2B33D (needs you), --fault #E5484D (error), --off #7C8088 (empty or finished)
  Props (objects in the room, never chrome): --roof #8E887C, --window #AFD3E6, --monitor #2B2E33 / #40444C / #1F2226, --cork #C99A5B / #8C6436, --brass #C9A45C / #8A6D2E, sticky notes #F7E27A / #9FD8F2 / #F6B5C8, --terminal #16181C with #E8E6E1 text
  People: shirts, skins and hair from fixed palettes in src/lib/looks.ts, picked per agent id
Type:
  Signage: "DIN Condensed" (macOS system font, bold), uppercase allowed only on physical signs (roof sign, floor nameplates). Fallback "Arial Narrow", sans-serif.
  Numerals: "DIN Alternate" bold for counts and floor numbers (tabular). Fallback the mono stack.
  Interface text: the macOS system face (-apple-system). Recorded decision: it is a native desktop app and the chrome speaks the platform's voice.
  Code, paths, terminal: "SF Mono", Menlo, monospace.
Motion:
  Characters: looping CSS animations that encode session state (typing, hand-up wave, thinking dots, idle breathing, finished slump). A stuck agent (error) lies flat on the floor in front of the desk with X for eyes and a blinking red "!" (owner request, 2026-09-29). Semantic, not decorative.
  Navigation: one camera move per action (dive into a floor, pull back to the building), 450-600ms tween, ease-out, no bounce, no overshoot.
  Agent-to-agent messages: a worker walks a note to the recipient's desk.
  prefers-reduced-motion: character loops stop, camera moves become cuts, walkers teleport.
Signature device: the metaphor made structural. Every sidebar section is an object in the room and opens its panel: memo pile on the manager's desk = Decisions queue, cork board = Human TODO, whiteboard = AI TODO, meter on the wall = Spend, brass plaque = Info, clipboard sign-out sheet by the door = Signed out (sessions to resume, Antek 2026-10-01), an agent walking a note to a colleague = agent-to-agent message.
First-paint emotion: command-play (a management sim: you are in charge, and it is a toy you want to poke). Owner picked "Command and Play"; recorded as one compound.
Voice: matter-of-fact British English. States in human words: Working, Thinking, Needs you, Stuck, Idle, Finished. Errors say what happened and what to do next. No em dashes in interface copy.
Dark mode: deferred (daylight only)
Hard-no exceptions: none
Rejected directions: not chosen on 2026-09-29 (drafts page): B night tower, C architect's axonometric, signature 1 lift ride, signature 3 16-colour pixel constraint. Owner may revisit; agents do not re-propose them.
Implementation constraints:
  CSS 3D rig. Building: rotateX(68deg) rotateZ(18deg), shallow storeys (depth 80, height 56) with desks near the front so the slab above never hides a worker. Single floor: rotateX(60deg) rotateZ(45deg), no ceiling.
  Billboards (people, bubbles, nameplates) counter-rotate with rotateZ(-z) rotateX(-x) so they always face the camera.
  Tauri renders in WKWebView: every scene change must be checked in WebKit, not only Chromium.
  Minimum window 900x600 (tauri.conf.json). That is the small-viewport floor. The 320px web floor does not apply to this desktop app.
Hard nos (project-local additions): emoji as icons; internal enum names in the interface (waiting-human, raise-window); a panel overlapping another panel; search that removes storeys from the building (non-matching floors dim instead).

## Gate record (2026-09-29)

- Identity base: offered inherit house-style / pop alternate (Leisure OS) / diverge fully. Owner: diverge fully.
- Concept derivation: offered as live drafts: A SimTower cutaway, B night tower, C architect's axonometric. Owner: A.
- Signature device: offered as live drafts: 1 lift ride (bespoke effect), 2 metaphor to the pixel (structural metaphor), 3 16-colour PICO-8 pixel grid (severe constraint). Owner: 2.
- First-paint emotion: offered command, play, calm, precision. Owner: command and play.
