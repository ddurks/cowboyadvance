# Range — design spec (draft)

Status: draft for review, no code yet. Supersedes the level-based "Sundown Drives"
design on the `game-loop` branch as the direction for the game.

## Guiding principles

1. **Simple and straightforward.** One input (ride). A handful of rules, each
   visible on screen. If a rule needs a tutorial, cut it.
2. **Quick to join, quick to leave.** A run is a few minutes, starts the moment
   you ride out, and ends whenever you choose to ride into the corral. Nothing is
   lost by leaving except the herd you were still carrying.
3. **Players and bots are interchangeable.** A rider is a rider. Bots obey
   exactly the rules a human does and drive the same input slot, so a human can
   replace a bot (or a bot a human) without the world noticing. v1 ships with
   bots only, but nothing may assume that.

## The pitch

A round range of open grassland with a corral in the middle. Ride out, push wild
cattle together into a herd, and bring it home through one of the corral's
gates. What you pen is your score and joins your ranch forever. Out on
the range, rustlers are doing the same thing — and any of them will ride
through your herd to scatter it if it's worth their while.

## A run

- You start at a corral gate with no cows. The run starts when you ride out.
- Gather: push wild cows into contact with you or your herd and they join it.
- Cash out: drive the herd into the corral through any gate. Penned cows are
  safe. **The run ends when you yourself ride through a gate**; cows still
  outside are lost to the range (they turn wild).
- There is **no bust and no timer**. Having 0 cows or 100, you keep playing
  until you choose to cash out. Run length comes from how greedy you are;
  target 3–5 minutes.
- Score for the run = cows penned.

## Core rules

These are the whole game. Each maps to a pure function in the sim core.

### Riders
- One stick, direction only: past a small deadzone the rider gallops that
  way, exactly like the keys, so touch and keyboard play the same. Easing in
  comes from the horse's acceleration (tapping). No turning radius.
- Every rider has an id; every owned cow points at its rider's id.

### Ownership
- A cow is either **wild** or **owned** by exactly one rider.
- A calm wild cow joins rider R's herd when it comes within `JOIN_RADIUS` of R
  or of any calm cow owned by R. Ties go to the nearest.
- Owned cows flock with their own herd only (cohesion/alignment ignore other
  herds), so herds stay distinct even when they brush past each other.

### Herding
- Herding is pushing, as today: cows move away from riders. Wild cows drift
  away calmly. Your own herd yields to you the same way — you steer the herd by
  riding behind and beside it.
- **Momentum scales with herd size.** Small herds are nimble; past ~20 cows
  they accelerate and turn slowly and coast. Implemented as herd-size-scaled
  steering inertia on owned cows, not a separate physics model.

### Panic (the only danger)
- Only **other** riders cause panic. A rider coming within `SPOOK_RADIUS` of a
  cow owned by someone else spooks it.
- A spooked cow becomes **wild**, bolts away from the intruder, and is
  **unclaimable while panicked**.
- Panic **ripples outward**: a panicked cow spooks its own former herd-mates
  within `CONTAGION_RADIUS` after a short delay, with a decaying chance per
  hop. Tight herds cascade fully; loose herds only partly.
- Panic decays over a few seconds. Once calm, the scattered cows are ordinary
  wild cows: whoever pushes them into their herd first gets them — raider,
  victim, or a third rider.
- Penned cows (inside the corral) can never be spooked.

### Corral
- Just a corral at the centre of the range (no town in v1; one can grow
  around it later), with several gates (one per compass side)
  so you can arrive from any direction. Gate geometry reuses today's
  fence-and-gap level code; threading a big, heavy herd through it is the
  finale of every run.

## The range

- **Shape:** round, bounded by cliffs/river — a natural barrier that behaves
  like a fence physically.
- **Size:** ~30 s at full gallop from the corral to the edge. `RANGE_RADIUS` is a
  tuning constant derived from that and `PLAYER_SPEED`.
- **Population:** ~120 cows on the range at all times.
- **Initial layout:** strays of 1–3 near the corral, wild herds of 10+ near the
  edge. Generated deterministically from a hash, like today's spawns.
- **Replenishment:** every banked cow is replaced by a new wild cow placed
  evenly across the range, away from riders. The range never empties.
- **Persistent:** the range carries on between your runs. Bots keep their
  herds; you return to the same living world at a corral gate.
- **Look:** current toy-like crisp style and sunny day, extended to the bigger
  world. No new terrain types in v1.

## Rustlers (bots)

- **Count:** crowded, ~10–12 riders.
- **Same rules as a human**: they gather, they raid, they get raided. A bot is
  a brain that writes into a `PlayerInputs` slot; it reads the same world state
  a human sees.
- **Brain:** each tick, score the reachable opportunities — wild groups to
  gather vs. other herds to raid (players' *and* bots') — weighted by size and
  discounted by distance, and ride at the best one. They prefer raiding **big**
  herds, so small herds (and struggling players) are mostly left alone: natural
  rubber-banding with no mercy mechanic.
- **Bots never bank.** Their herds stay on the range as raidable stockpiles,
  which makes big bot herds the prizes on the outskirts over time.

## Joining and leaving (design now, build later)

Rider slots are fixed (e.g. 12). Every slot is always filled by a human or a
bot.

- **A human joins:** they take over a bot's slot. That bot's herd calms into
  wild cattle where it stands; the human starts at a corral gate with nothing.
- **A human leaves mid-run:** a bot takes over their rider in place, herd and
  all. The world never has a hole in it.
- **A human cashes out:** their run ends, the slot hands back to a bot (or
  their next run, if they ride out again).

v1 implements this only as the slot abstraction (`Rider { controller: Human |
Bot }`) with one human; online join/leave comes with netcode.

## Screens and HUD

- **Run:** the camera follows your rider and pulls back up to ~2× as your herd
  grows (smoothed so it never pumps). HUD shows **only your herd count**.
  Minimap in the cabinet frame: you + your herd, rustlers and their herds,
  wild cattle, the corral. An edge arrow points to the corral when it's off screen.
- **Cash-out → Ranch:** a 3D pasture where every cow you've ever banked
  grazes (rendered count capped for performance; true number shown in the
  frame), with this run's score, your best run, and your ranch total. One tap
  rides out again.

## Scores and persistence (all local in v1)

- Best single run (cows banked) and ranch total. Ranch cows are never lost.
- Saved on device: ranch total, best runs, and the full range state (cow
  positions and owners, bots and their herds) — a few KB.
- No breeding, upgrades, horses, or online leaderboard yet.

## Architecture notes

Respects the existing layering (see CLAUDE.md):

- **Pure core (glam only, unit-tested):** ownership/join, panic contagion,
  momentum scaling, bot brain, range generation and replenishment, rider-slot
  takeover, save-state (de)serialisation of the range. Likely new modules:
  `range.rs` (world layout, spawns, replenishment), `herd.rs` (ownership,
  panic), `bot.rs` (brain).
- **Determinism:** no RNG anywhere. Spawns and respawns hash off
  (world seed, tick, slot); the bot brain is a pure function of world state.
  Persisting the range must round-trip exactly.
- **Multiplayer seam:** bots and humans both fill `PlayerInputs`; nothing
  special-cases player 0. Online play is expected to be host-authoritative
  (drop-in/drop-out suits it better than lockstep).

### Technical work this implies
- **Spatial grid** for boids, joining and panic — today's all-pairs pass won't
  scale to ~120 cows + 12 riders on phones/web.
- **Follow camera with zoom** — today's camera and cabinet framing assume a
  fixed play box.
- **Grass and ground over a large world** — today's grass is sized to one
  field; needs chunking or a range-wide scheme that stays within WebGL2 limits.
- **Animation cost** — ~120 skinned cows; may need cheaper animation for
  off-screen or distant cows.
- **Local save** on desktop, iOS and web (web needs browser storage) without
  heavy dependencies.

## Prototype plan

Each step is playable on its own; we judge it by playing it before moving on.

1. **Big range.** Round world with cliff edge, follow camera, spatial grid,
   ~120 wild cows, stick riding. *Check:* riding around and pushing cows feels
   good at this scale and runs smoothly on web.
2. **Gather and cash out.** Ownership/joining, herd-only flocking, momentum,
   central corral with several gates, run end on riding in, replenishment.
   *Check:* a solo run with no rivals is already a satisfying 3–5 minutes.
3. **Rustlers and panic.** Bots, spooking, ripple contagion, scatter and
   reclaim. *Check:* raids feel fair and readable; big herds feel at risk,
   small ones don't.
4. **Ranch and persistence.** Ranch pasture screen, local scores, persistent
   range. *Check:* coming back to the same world and seeing the ranch grow
   pulls you into another run.
5. **Readability.** Minimap, edge arrow, zoom tuning.

## Open questions (to settle by playtesting)

- **Herding model:** this spec keeps today's push-to-herd for your own herd.
  The alternative (your herd follows you) is simpler to steer but loses the
  core mechanic; confirm push.
- **Range size vs density:** 30 s at full gallop is a very large area for 120
  cows. If the range feels empty, shrink the radius before adding cows.
- **Panic numbers:** spook radius, contagion radius, delay, hop decay, calm
  time.
- **Momentum curve:** at what herd size it starts to bite, and how hard.
- **Bot brain weights:** gather vs. raid, size preference, how aggressively
  they target big herds.
- **Layout drift:** respawns land evenly and bot herds grow over time, so the
  initial strays-near-the-corral / big-herds-far-out spread will fade. Watch whether
  that matters.
- **Existing collisions:** today riders can be thrown by cows (ragdolls). Keep
  as-is, soften, or drop for simplicity?
- **Corral gate width** relative to today's gate, given heavy herds.

## Not in v1

Online play, coop / shared herds, breeding, upgrades and horses, online
leaderboards, terrain obstacles, weather, day/night, audio pass.
