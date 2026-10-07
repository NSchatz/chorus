# P12: Enclosures per speaker class (wood, printed or mixed)

- Decisions: K88
- Status: ACCEPTED (2026-10-07, the owner): Option C as written, every class's recommendation unchanged (`docs/decisions/0242-p12-accepted-enclosures-per-class.md`); first put forward 2026-10-06. The printed parts (the compact, the two-way's fittings) are PETG for now, the devices repo's decision (the ADR says why)
- If deferred: No enclosure is built. The compact acoustic design uses this file's compact answer (printed ASA, 8 mm wall, the volume and leakage tolerances below) marked ASSUMED, and the other classes' designs assume 18 mm wood panels
- Builds on: goal 24 (§28 item 1, "P12 (K88): enclosures per class, written here and marked PROPOSED"; item 2, the compact speaker's "enclosure per P12's recommendation"), goal 25 (§29, the two-way and the subwoofer), goal 26 (§30, the soundbar and the LCR set), the amendment of 2026-10-01 item 3 (P12 stays chorus's proposal; the enclosure models are the owner's devices repo's) and item 7 (the acoustic design record names "the tolerances an enclosure must hold" and P12's path)

## Question

For each speaker class, is the enclosure wood from the owner's shop, 3D-printed on the owner's
printer, or a mix of the two? The answer is argued from acoustics (panel stiffness and resonance,
net volume, sealing, the port), the finish, the cost and what the owner can make. The compact
class is answered concretely enough for its acoustic design to use: the material, the wall
construction, and the tolerances on net volume and leakage that the material can hold.

The owner's decisions that bound it:

- K88: "**Research and propose**: per class, wood from the owner's shop vs 3D-printed vs mixed,
  from acoustics (panel resonance, volume), finish, and the shop and printer the owner has. Not
  chosen as final: mixed, wood everywhere, printed where possible."
- K22: the classes are the compact smart speaker ("Era 100-class, ESP32-S3 + TAS58xx, PoE"), the
  active two-way, the subwoofer and the streaming amp; "physical builds and orders are the
  owner's". K72 adds the theater front: a soundbar and a separate LCR set.
- K89: "**Under ~$150** per compact smart speaker (electronics, driver, amp, enclosure materials;
  not labour)". The enclosure's material is inside that budget.
- K90: PoE+ for the compact speaker; mains for the two-way, the subwoofer and the soundbar.
- The amendment of 2026-10-01, item 3: chorus designs the acoustics and writes P12; the enclosure
  models, drawings and print files are made in the owner's devices repo from chorus's request.
- The owner's room list (answered 2026-10-04): two 5.1 rooms (two-way L, C and R and a subwoofer
  on mains, compact surrounds on PoE+), a kitchen compact pair, one bathroom compact, a garage
  compact pair, and an outdoor pair on a deck. The rack amp is dropped.

What P12 does not decide: drivers, box volumes and alignments (the acoustic designs), any model,
drawing or print file (the devices repo), and any order or print (the owner's).

## Constraints that bind every option

- **Fitness only (CLAUDE.md rule 8).** K88 itself names "the shop and printer the owner has" as an
  input, so what the owner can make is a criterion here by the owner's decision. It is used as a
  capability question (can this part be made to the tolerance the acoustics need), never as a
  convenience argument for a software choice.
- **Three rooms are hard on materials.** The bathroom is humid; the garage is unconditioned (hot
  in summer, cold in winter: **ASSUMED**, its temperature range is not recorded); the deck is
  outdoors (rain, sun, the full temperature range). A material that swells with moisture or
  softens in the sun is out for those rooms whatever its acoustics.
- **The compact speaker holds electronics.** The endpoint board, the amplifier, the PoE power
  stage, the controls and the microphone with its hardware mute all live in or on the box (K22,
  K67), so the enclosure needs board mounts, a cable entry that seals, a microphone port and a
  removable panel that reseals.
- **Hardware is read-only for agents (K4).** Nothing here orders, prints or cuts. Every tolerance
  below is what a material can be expected to hold, to be confirmed on the first build.
- **A box only has to be stiff, tight and the right size.** The acoustic design fixes the net
  volume, the port and the baffle. The enclosure's job is to hold that volume, to leak little
  enough, and to keep its walls from radiating sound of their own. Those three are the criteria.

## What the acoustics ask of a wall

All figures here are read from the sources listed at the end (2026-10-06) or calculated from them;
a calculation says so.

**Stiffness and panel resonance.** A flat wall panel is a plate. For a simply supported
rectangular plate of thickness t and sides a and b, the first resonance is
f = (π/2) · sqrt(D / (ρ·t)) · (1/a² + 1/b²), with D = E·t³ / (12·(1 − ν²)) (the plate formula on
the vibration-of-plates page, rewritten for the full thickness). So the frequency rises with
thickness, with sqrt(E/ρ), and with the inverse square of the panel's span. Span is the strong
term: halving the unbraced span quadruples the frequency. Real walls are glued at the edges, which
is stiffer than simply supported, so the figures below are low estimates. Poisson's ratio is
**ASSUMED** 0.30 for the wood panels and 0.35 for the plastics.

| Material (source) | E, GPa | Density, kg/m³ | sqrt(E/ρ), m/s (calculated) |
|---|---|---|---|
| MDF, moisture-resistant grade (maker's data sheet) | 3.79 | 769 | 2220 |
| MDF, generic (wood handbook, Table 12-1) | 3.59 | 700 to 900 | 2000 to 2260 |
| Birch plywood, 18 mm 13-ply (Finnish plywood handbook) | 10.05 along the face grain, 7.45 across | 680 | 3840 along, 3310 across |
| Printed PLA (filament maker's data sheet, flexural, 100% infill) | 3.1 | 1240 | 1580 |
| Printed PETG (same maker) | 1.7 | 1270 | 1160 |
| Printed ASA (same maker) | 2.0 | 1070 | 1370 |

Calculated first resonances (simply supported, so low estimates):

| Panel | First resonance |
|---|---|
| 18 mm MDF, 400 x 300 mm unbraced (a subwoofer or two-way side) | 330 Hz |
| 18 mm birch plywood, 400 x 300 mm unbraced | 490 to 570 Hz |
| 18 mm MDF, braced to 200 x 200 mm | 950 Hz |
| 18 mm birch plywood, braced to 200 x 300 mm | 1020 Hz |
| 12 mm birch plywood, 150 x 150 mm (a compact wall) | 1680 Hz |
| 12 mm MDF, 150 x 150 mm | 1130 Hz |
| 8 mm ASA, 150 x 150 mm, no rib | 470 Hz |
| 8 mm ASA, one rib (150 x 75 mm) | 1180 Hz |
| 8 mm ASA, ribbed to 75 x 75 mm | 1880 Hz |
| 8 mm PETG, 150 x 150 mm, no rib | 400 Hz |
| 8 mm PLA, 150 x 150 mm, no rib | 540 Hz |
| 8 mm ASA, 300 x 350 mm, no rib (a two-way side, if printed) | 100 Hz |

What the table says: plastic is the weaker wall material (sqrt(E/ρ) about half of MDF's and a
third of birch plywood's), and a printed wall is also thinner. A small box hides that, because
its spans are short and a printer adds ribs for free: an 8 mm ASA wall ribbed to 75 mm sits as
high as a 12 mm birch plywood wall of a compact box. A large box does not: a printed 300 x 350 mm
side would ring at about 100 Hz, in the middle of the woofer's band, and bracing it back up costs
more plastic than the wood it replaces.

**Damping matters as much as stiffness.** A BBC research report of 1977 on cabinet walls built
two cabinets of 63 x 30 x 30 cm in birch plywood, one with walls "twice the thickness, i.e. 18 mm",
and found "the output from the thick walled cabinet is actually greater" because "the Q is
greater"; with damping layers added the thin cabinet met its criterion and the thick one did not.
A designer's published notes ask that "no un-braced box panel area should be larger than 4 inch
squared for 3/4 inch thick wood panels" and that a "constrained layer that dissipates energy will
reduce Q". A trade article measured the back panel of "a 50 ltr box, made of 18mm chipboard" at
"peaks 10dB lower than the driver output" and gives the rule "up to 30 ltr ... at least 18mm; from
30 to 50 ltr 25mm and above 50 ltr ... at least 30mm". No loss factor was found for MDF, plywood
or any printed plastic, so no option below is argued from a damping number: every class gets
bracing (or ribs) plus a damping layer or pad on its largest panels as an open item to settle by
measurement on the first build.

**Net volume.** The box must hold the net volume the acoustic design sets. A vented box's tuning
follows the cavity-and-neck resonator formula, f = (v/2π)·sqrt(A / (V·L)), so a volume error of
x percent moves the tuning by about x/2 percent (calculated). A closed box's resonance is
fc = fs·sqrt(1 + Vas/Vb) (the standard closed-box relation; no page was read for it here:
**ASSUMED**), so it moves by less than x/2 percent. The 1973 vented-box paper draws its
misalignment charts for tuning errors of ±20% and ±50%: a box within a few percent of its volume
is far inside what the alignment tolerates. The driver's own tolerances are larger (**ASSUMED**;
the acoustic designs state them).

What each material holds:

- Printed: a 3D-printing service's guide gives desktop FDM "± 0.5% (lower limit: ± 0.5 mm)" and
  "Shrinkage usually occurs in the 0.2 - 1% range"; a second service gives "± 0.5% with a lower
  limit of ±0.020 in." On a 144 mm inside dimension that is ±0.7 mm per axis, so ±1.5% of volume
  at worst (calculated). Shrinkage is systematic and is taken out by scaling after one test print.
- Wood: a sheet maker's data sheet gives thickness "±0.005 in ±0.125 mm". A cut made on the
  owner's table saw or track saw is **ASSUMED** good to ±0.5 mm (the saw's type and fence are not
  recorded; no authoritative figure was found). On the same 144 mm box that is ±1.0% of volume,
  and on a 15 L or 50 L box well under ±1% (calculated).

Either way the wall material is not what limits volume accuracy. What does is the volume the
driver, the electronics, the braces and the port displace, which is the enclosure model's sum.

**Sealing.** The 1973 vented-box paper: "Leakage losses are usually the most significant, giving
[Q_L] values of between 5 and 20", in enclosures that "were well built and appeared to be quite
leak-free", with some leaks "traced to the drivers"; "most commonly measured values of QB are in
the range of 5-10". A tutorial on vented alignments calls Q_L = 7 "a normal amount of leakage".
So a design assumes Q_L = 7 and a good box does better.

- Wood: glued joints in sheet goods are airtight (**ASSUMED**; standard practice, no figure
  read); leaks come from the removable panel, the driver gasket, terminals and cable entries.
- Printed: walls can be porous. A printer maker's test on watertight prints recommends walls of
  "2-3 mm" as "a good start" and "four perimeters and 60% infill"; its untreated PETG part still
  leaked and an epoxy coat gave "perfect watertightness". A published parametric printed-speaker
  design ("7mm walls", "20% infill and 3 shells", PLA, a 3 in driver "in a 1.7 liter enclosure",
  "about 17 hours to print") calls its boxes "almost certain to be air-tight"; another builder
  writes that printed parts "tend to be low in mass and not very stiff. Air-tight walls are
  sometimes difficult to achieve". Water under pressure is a harder test than a speaker's few
  hundred pascals, so thick multi-perimeter walls are expected to be tight enough, with an inside
  coat as the fallback: to be shown by the first box's impedance curve.

**The port.** A port wants a smooth flared mouth and an exact length. Printed, both come free and
repeat exactly; in wood a port is a bought tube or a slot built from panels. This favours a printed
port (as a part or as the whole box) in every class that has one.

**Moisture, heat and sun.**

| Material | Moisture | Heat and sun |
|---|---|---|
| MDF | Standard and MR10 grades: 24-hour thickness swell "5.5% or less"; an MR50 board "Thickness Swell 3%". A humidity-cycling study measured thickness change up to 0.56 mm and internal bond down 22 to 23% | Not a limit indoors |
| Birch plywood | Exterior-glue birch plywood "fulfils the requirements of EN 314-2 class 3 exterior", yet "must be properly surfaced, edge sealed" outdoors; thickness grows "0.3-0.4 % increase per 1 % increase of moisture level". One retailer's 18 mm Baltic birch is graded "Interior" | Not a limit indoors |
| PLA | 0.19% absorption (7 days) | Heat deflection 55 °C; "gets soft and deforms at temperatures over 60 °C"; "degrades under UV light" |
| PETG | 0.10%; "Water and humidity resistant" | Heat deflection 68 °C; "most exterior use (with temperatures below 80 °C)" |
| ASA | 0.17% | Heat deflection 93 °C; "excellent UV resistance ... perfectly suitable for making outdoor parts with a long lifespan" |

So MDF is out for the bathroom, the garage and the deck; plywood needs an exterior glue line and
every face and edge sealed to live there; PLA is out for the garage and the deck (a dark part in
the sun or a closed garage in summer is **ASSUMED** to pass 55 °C) and is marginal next to an
amplifier anywhere; ASA is the one material here made for all three rooms.

## What the owner can make

Shop and printer facts are the owner's inventory records (the tool, spool and material registers,
read 2026-10-06). They have no public URL; where the record itself is marked assumed, or holds no
detail, the fact is **ASSUMED** here.

| Capability | Record says | For enclosures |
|---|---|---|
| Table saw | Owned; "Type (jobsite / contractor / cabinet) and fence quality are NOT recorded" | Rips panels to width; accuracy **ASSUMED** ±0.5 mm |
| Track saw | Owned (model, depth of cut and track length not recorded: **ASSUMED** usable for sheet breakdown) | Breaks down full sheets, straight finish cuts |
| Miter saw | Owned; whether it slides is not recorded | Crosscuts braces; panels only if it slides |
| Router | Owned (confidence assumed; base type, collet size and bits not recorded: **ASSUMED**) | Driver cut-outs and recesses with a template, rabbets, round-overs. A circle jig or templates are not recorded: **ASSUMED** absent; templates can be printed |
| Jigsaw | Owned (model not recorded: **ASSUMED**) | Rough cut-outs only; the record notes its cut "is not square enough for a glue joint or a finished edge without cleanup" |
| Jointer | Owned (bed width not recorded: **ASSUMED**) | Not needed for sheet goods |
| Sander | "Random-orbit sander and/or router": which one is not recorded (**ASSUMED** a sander is to hand) | Finish prep |
| Clamps, bench, sawhorses | Owned; "Clamp count and capacity are NOT recorded" | Glue-ups; enough clamps for a 400 mm subwoofer box are **ASSUMED** |
| Pocket-hole jig, compressor and nailers, edge-band iron | Owned | Carcass assembly; iron-on banding for plywood edges |
| Thickness planer, band saw, dowel jig, loose-tenon joiner | Not owned | No solid-wood panels, no curved resawn parts: sheet goods with butt, rabbet or pocket-screw joints |
| Painting kit (rollers, brushes, trays) | Not owned | A painted finish needs a small purchase; a wipe-on clear finish on plywood needs a rag (**ASSUMED**) |
| Digital calipers, digital scale | Owned (models not recorded: **ASSUMED**) | Checking printed and cut dimensions |
| 3D printer | One machine, an enclosed-chamber printer whose maker lists a build volume of "256 × 256 × 256 mm", a "320°C Hotend", a "110℃ Heated Bed" and a "Fully Enclosed Chamber, High-Temperature Printing Beyond PLA" (https://us.elegoo.com/pages/elegoo-centauri-carbon, read 2026-10-06). That it prints ASA well at 160 mm part sizes is **ASSUMED** until a test print | Parts up to about 250 mm on a side in one piece |
| Filament in hand | Five 1 kg spools, all PLA (recorded cost $14.49 to $15 each, marked assumed in the records) | No ASA or PETG is in hand: a purchase |
| Sheet goods | None in stock. The records price a local yard's sheet (2026-10-03): 3/4 in MDF $35, 1/2 in MDF $28.25, 3/4 in white birch veneer-core plywood $70, 1/2 in $67, per 4 x 8 ft sheet; wood glue $9.35 per 16 oz | Every sheet is a purchase |

Public prices read 2026-10-06 (URLs under Sources):

| Item | Price |
|---|---|
| 3/4 in MDF, 4 x 8 ft | $59.00 (a hardwood dealer), $76.88 for 49 x 97 in (a lumber yard) |
| 3/4 in Baltic birch, 13-ply, 4 x 8 ft | $91.59 |
| 18 mm Baltic birch, 13-ply, 5 x 5 ft | $118.88 |
| 3/4 in Baltic birch, 30 x 30 in | $45.99 |
| ASA filament, 1 kg | $24.99 (one maker); $25.92 for 800 g (another) |
| PETG filament, 1 kg | $18.99; $29.99 |
| PLA filament, 1 kg | $15.99; $19.99; $29.99 |

Material per enclosure, for illustrative sizes only (**ASSUMED**: a 3 L compact, a 15 L two-way,
a 50 L subwoofer, a 1000 x 100 x 120 mm soundbar; the acoustic designs set the real volumes).
Calculated from the prices above (the $59.00 MDF sheet, the $91.59 birch sheet, ASA at $24.99
per kg):

| Enclosure | Wood | Printed |
|---|---|---|
| Compact, 3 L net | 12 mm birch plywood, about 0.15 m²: under $6 (priced at the 18 mm sheet's rate; a 12 mm public price was not read: **ASSUMED** no higher). Outside about 168 mm cube | 8 mm ASA walls at four perimeters and 60% infill: about 0.9 kg, about $23. Outside about 160 mm cube. Print time **ASSUMED** 20 to 30 hours (the published 1.7 L design took "about 17 hours") |
| Two-way, 15 L net | 18 mm birch plywood, about 0.54 m² with a brace: about $17 (MDF about $11) | About 3.2 kg, about $81, in at least two bonded pieces because a side exceeds 256 mm |
| Subwoofer, 50 L net | 18 mm MDF, about 1.3 m² with a doubled baffle and braces: about $26 (birch about $40) | Not printable on this machine in fewer than many pieces |
| Soundbar | 12 mm birch plywood, about 0.5 m²: about $16 | Longer than the build volume four times over |

Cost does not separate the options for the compact class: under $6 against about $23, inside a
$150 budget (K89). It does for the larger classes, where printing costs several times the wood
and gives a worse wall.

## Options

### Option A: wood everywhere

- What: every class is cut from sheet goods in the owner's shop. Compacts in 12 mm birch plywood,
  the two-way, LCR and soundbar in 12 to 18 mm birch plywood, the subwoofer in 18 mm MDF. Printed
  parts are not used.
- Costs: one or two sheets cover the whole room list (eleven compacts need about 1.7 m², little
  more than half a sheet: calculated). Labour is the owner's: eleven small boxes each with six
  panels, a driver cut-out, a removable panel, board mounts and a microphone port.
- Risks:
  - The bathroom, the garage and the deck. Interior-grade plywood and MDF do not belong there;
    an exterior-glue plywood with every face and edge sealed is a different sheet and a real
    finishing job on a 160 mm box, and a failed edge seal swells.
  - The compact's fittings (board standoffs, a sealed cable entry, button and LED openings, the
    microphone's port and mute switch, a flared port) are all hand work in wood, repeated eleven
    times.
  - Driver cut-outs and recesses need router templates the records do not show.
- Fit: the best walls acoustically in every class; a poor fit for the compact's rooms and for the
  compact's small, fiddly, repeated box.

### Option B: printed where possible

- What: every enclosure that fits the printer is printed: all compacts, and the two-way, LCR and
  soundbar as bonded sections. The subwoofer stays wood.
- Costs: about $23 of ASA per compact; about $81 per two-way or LCR cabinet (six of those across the
  two 5.1 rooms, and three more for an LCR set), plus days of printing each
  (**ASSUMED** over 60 hours per two-way cabinet, scaled from the compact).
- Risks:
  - Large printed panels are the weak case: an unribbed 300 x 350 mm ASA wall rings at about
    100 Hz (calculated above), and every bonded seam is a leak and a crack line. ASA's interlayer
    adhesion is the lowest of the three plastics (11 ± 1 MPa against 17 ± 3 for PLA).
  - Warping of ASA grows with part size (**ASSUMED**; to be shown by test prints).
- Fit: right for the compact; wrong for anything bigger than the build volume, where it costs more
  and performs worse than wood.

### Option C: mixed, by class (recommended)

- What:
  - **Compact:** printed ASA, one piece plus a gasketed panel (details in the next section).
  - **Active two-way and the LCR set:** 18 mm birch plywood (13-ply, void-free), braced so that no
    unbraced span is over about 200 mm, with printed fittings: the port and its flare, the
    electronics carrier, the tweeter's waveguide if the design wants one, and the router templates
    for the driver cut-outs. Finish: clear coat on the birch faces, edges banded or left as plies.
  - **Subwoofer:** 18 mm MDF, a doubled (36 mm) baffle, window braces so that no unbraced span is
    over about 200 mm, painted; a printed port flare if the design is vented. Birch plywood is the
    alternative at about $14 more per box.
  - **Soundbar:** a 12 mm birch plywood shell with a divider between every driver's chamber, and
    printed internals and top insert (the touch surface, status LED and microphone mount of K72,
    the ports). 12 mm is **ASSUMED** enough because the dividers keep every span short; the
    soundbar's design confirms it.
  - **Rack amp:** no enclosure is proposed; the owner dropped the rack amp on 2026-10-04.
- Costs: about $23 of ASA per compact (about 10 kg, about $250, for the eleven on the room list:
  calculated); for the wood classes less than one sheet of birch plywood and one of MDF per 5.1
  room (calculated from the illustrative sizes), plus a finish. A painting kit is a small purchase.
- Risks:
  - ASA on the owner's printer is unproven; the fallback is PETG for the indoor rooms (heat
    deflection 68 °C) with ASA kept for the garage and the deck.
  - Printed walls may leak or ring more than calculated; the first box is measured before ten
    more are printed (open items).
  - Two materials and two workflows to keep going instead of one.
- Fit: each class gets the wall that suits its size. Small spans make a printed box stiff enough,
  and printing answers the compact's rooms, fittings and repetition; large spans, high pressure
  and furniture-grade faces go to wood, which the owner's saws and router cover.

## The compact class, concretely

This is the part the compact acoustic design uses. Every number is a starting value for the
enclosure request, confirmed on the first printed box.

- **Material:** ASA for every compact (bathroom, garage, deck, kitchen and the surrounds), so one
  model and one print profile serve all eleven. PETG is the fallback for the indoor rooms only.
  PLA is not used: heat deflection 55 °C, next to an amplifier and a PoE power stage.
- **Wall construction:** 8 mm walls printed with four perimeters on each face and at least 60%
  infill between them (the settings a printer maker's watertightness test recommends, at more than
  twice its wall thickness). The baffle is 10 mm with the driver recessed and a sealing land for
  its gasket (**ASSUMED** dimensions for the enclosure model to check against the driver).
- **Ribs:** no flat wall span over 80 mm without a rib or a change of plane, which puts the first
  panel resonance of an 8 mm ASA wall near 1.7 kHz or above (calculated for a simply supported
  80 x 80 mm panel, treating the wall as solid; four perimeters per face carry about 83% of a
  solid wall's bending stiffness at about 78% of its mass, calculated, with the infill **ASSUMED**
  to carry the shear). Edges and corners rounded outside, at least 10 mm radius, which also eases
  the baffle's diffraction step.
- **One body, one panel:** the body prints in one piece (about a 160 mm cube for a 3 L box, inside
  the 256 mm build volume), so the only seam is the removable panel. That panel seals on a
  closed-cell foam gasket in a printed groove and fastens with machine screws into threaded metal
  inserts (**ASSUMED** practice; the enclosure model picks the parts).
- **Openings:** the cable entry, the controls and the microphone port each seal with a gasket or
  sit in a chamber walled off from the acoustic volume, so that none of them is a leak path.
- **Tolerance on net volume:** ±2% of the designed net volume (the printing services' ±0.5% per
  axis gives ±1.5% at worst on a 144 mm cavity; shrinkage is scaled out after the first print).
  That moves a vented box's tuning by about 1% and a closed box's resonance by less.
- **Tolerance on leakage:** the box holds Q_L of 7 or better. The acoustic design assumes
  Q_L = 7 and must not depend on a tighter box; a built box is accepted when its measured
  impedance curve shows Q_L of 7 or more. If the first box fails, the fallback is an inside coat
  of epoxy, then five perimeters.
- **Port, if the design is vented:** printed in one piece with the body, flared at both ends, its
  length held to the same ±0.5 mm.
- **Damping:** the cavity's fill is the acoustic design's. A mass or constrained layer on the
  largest walls is added only if the first box rings on measurement.
- **For the deck:** the same ASA box. P12 asks the acoustic design for a closed box or a
  downward-facing screened port there, and a driver rated for weather; both are the design's
  choice, not this proposal's.
- **Cost against K89:** about $23 of ASA per speaker (about 0.9 kg at $24.99 per kg), leaving
  about $127 for the driver, the amplifier and the electronics.

## Comparison

| Criterion | A: wood everywhere | B: printed where possible | C: mixed by class |
|---|---|---|---|
| Compact wall, first resonance | 12 mm birch, 150 mm span: about 1.7 kHz | 8 mm ASA ribbed to 80 mm: about 1.7 kHz | As B |
| Two-way and LCR wall | 18 mm birch braced to 200 x 300 mm: about 1.0 kHz | 8 mm ASA, 300 x 350 mm: about 100 Hz unribbed | As A |
| Subwoofer wall | 18 mm MDF braced to 200 mm: about 950 Hz | Wood (does not fit) | As A |
| Net volume held | About ±1% (cuts **ASSUMED** ±0.5 mm) | ±1.5% worst case, ±2% stated | Both, per class |
| Sealing | Glue joints tight (**ASSUMED**); panel and entries are the leaks | Walls may be porous; four perimeters, 60% infill, epoxy as fallback | Both, per class |
| Port | Bought tube or built slot | Printed, flared, exact | Printed in every class |
| Bathroom, garage, deck | Needs exterior-glue plywood, fully sealed; MDF is out | ASA: made for it | ASA compacts |
| Compact fittings (board, controls, mic, cable) | Hand work, eleven times | In the model, repeated by the printer | In the model |
| Finish | Clear coat or paint; a painting kit is not owned | As printed; layer lines show | Wood faces in the living rooms' large speakers; printed compacts |
| Material per compact | Under $6 | About $23 | About $23 |
| Material per two-way cabinet | About $17 | About $81 | About $17 plus printed fittings |
| What the owner makes it with | Table saw, track saw, router (templates not recorded) | The printer; ASA unproven on it | Both; templates printed |
| Labour | The most: eleven small boxes by hand | Printer hours, about a day per compact | Printer hours for compacts, shop time for eight or so cabinets |

## Recommendation

**Recommendation:** Option C, mixed by class: compact printed in ASA (8 mm walls, four perimeters, 60% infill, ribbed to 80 mm, ±2% net volume, Q_L of 7 or better); active two-way and LCR in braced 18 mm birch plywood with printed ports, carriers and templates; subwoofer in braced 18 mm MDF with a doubled baffle; soundbar as a 12 mm birch plywood shell with printed internals; no enclosure for the dropped rack amp.

Why: panel resonance falls with the square of the span, so plastic's lower stiffness costs nothing
in a ribbed 160 mm box and everything in a 350 mm one. The compact class is also where printing
pays for itself: eleven identical boxes, three of them in rooms that swell MDF and soften PLA, each
full of fittings that are free in a model and slow by hand. The large classes are where wood pays:
stiffer and better walls for a fifth of the material cost, in sizes the printer cannot make in one
piece, with faces that suit a living room. The owner gives up a single workflow, buys ASA (none is
in hand) and accepts that the printed box's sealing and ringing are shown by the first build, not
by this file.

## If the owner defers

No enclosure is requested as final. The compact acoustic design proceeds on the compact section
above with every value marked ASSUMED, and the two-way, subwoofer, soundbar and LCR designs assume
18 mm wood panels (12 mm for the soundbar) for their net-volume and baffle arithmetic. The
enclosure requests to the owner's devices repo carry "P12 is PROPOSED" and are re-read when the
owner decides; a decision for wood compacts would change the compact's outside size (about 168 mm
against 160 mm for 3 L) and its baffle edge radius, so its baffle-step arithmetic is redone then.

## Open items

- **The owner's decision:** taken 2026-10-07, Option C as written
  (`docs/decisions/0242-p12-accepted-enclosures-per-class.md`). Still open: whether the
  compacts in the living rooms
  may look printed (layer lines, one colour) or want a wood or fabric face.
- **ASA on the owner's printer:** unproven. One test print of a 160 mm ribbed box decides between
  ASA everywhere and PETG indoors; it also gives the shrinkage scale and the real print time.
- **Sealing of printed walls:** shown by the first box's impedance curve (Q_L of 7 or more), not
  by a source. The watertightness test read here is on smaller, thinner parts.
- **Ringing of printed walls:** no loss factor was found for any printed plastic, MDF or plywood.
  The first box is tapped and measured (an accelerometer or a near-field microphone on the wall)
  before the rest are printed; a mass or constrained layer is the remedy.
- **The outdoor pair:** which class it is (compact is **ASSUMED** here), its driver's weather
  rating and whether its box is closed are the acoustic design's.
- **The garage's temperature range** is not recorded; ASA's 93 °C margin is why it is chosen
  without it.
- **Shop facts not recorded** (each **ASSUMED** above): the table saw's type and fence, the track
  length, the router's base and collet, the clamp count, whether a sander is to hand, a circle jig.
  The owner's inventory asks for each by its own date.
- **Illustrative volumes** (3 L, 15 L, 50 L, a 1 m soundbar) are placeholders; every cost and
  resonance figure is recalculated when the acoustic designs fix the volumes.
  Since written (2026-10-06): the designs fixed the net volumes at 1.80 L for the compact
  (decision 0216), 9.29 L for the two-way (0217) and 73.54 L for the subwoofer (0229), and no
  soundbar is designed (0231), so its row and the soundbar enclosure above no longer apply. The
  figures here are not yet recalculated; that is part of the owner's decision on this proposal.
- **12 mm plywood** properties and price are taken from the 18 mm sheet (**ASSUMED**).
- **Finish:** a painting kit for the subwoofer and a clear finish for the birch are purchases the
  build packets list.
- **Exterior-grade wood** was not priced, since no wood enclosure is proposed for the bathroom,
  the garage or the deck.

## Sources

Every page was read on 2026-10-06.

- Plate vibration formula: https://en.wikipedia.org/wiki/Vibration_of_plates
- Cavity-and-neck resonator formula: https://en.wikipedia.org/wiki/Helmholtz_resonance
- Wood handbook, chapter 12 (MDF and plywood properties, Tables 12-1 and 12-6): https://research.fs.usda.gov/download/treesearch/62260.pdf
- Handbook of Finnish plywood (birch plywood modulus, density, glue class, moisture movement): https://koskisen.fi/wp-content/uploads/materials/Handbook-of-Finnish-Plywood.pdf
- Moisture-resistant MDF data sheet (density, modulus, swell, thickness tolerance): https://www.roseburg.com/resources/medex-technical-data-sheet-medford/
- MDF specification sheet (MR10 and MR50 definitions): https://www.buildgp.com/wp-content/uploads/2018/11/georgia-pacific-ultrastock-MDF-specifications-pdf.pdf
- Humidity cycling of wood-based panels: https://bioresources.cnr.ncsu.edu/resources/repeated-humidity-cyclings-effect-on-physical-properties-of-three-kinds-of-wood-based-panels/
- PLA data sheet: https://prusament.com/wp-content/uploads/2022/10/PLA_Prusament_TDS_2021_10_EN.pdf
- PETG data sheet: https://prusament.com/wp-content/uploads/2022/10/PETG_Prusament_TDS_2021_10_EN.pdf
- ASA data sheet: https://prusament.com/wp-content/uploads/2022/10/ASA_Prusament_TDS_2022_16_EN.pdf
- PLA material guide (softening, UV): https://help.prusa3d.com/article/pla_2062
- PETG material guide (exterior use, humidity): https://help.prusa3d.com/article/petg_2059
- ASA product page (UV resistance; $25.92 for 800 g): https://www.prusa3d.com/product/prusament-asa-jet-black-850g/
- Watertight printing test (perimeters, infill, epoxy): https://blog.prusa3d.com/watertight-3d-printing-part-2_53638/
- BBC research report 1977/3 on loudspeaker cabinet walls (text scan): https://archive.org/download/bbc-rd-reports-1977-03/1977_03_djvu.txt
- A designer's notes on cabinet panels and bracing: https://www.linkwitzlab.com/frontiers_2.htm
- Trade article on driver-induced cabinet vibration: https://audioxpress.com/article/speaker-design-driver-induced-vibrations
- Parametric printed speaker enclosures (wall, infill, print time): https://nothinglabs.com/speakergen-parametric-3d-printed-speaker-enclosures/
- Printed spherical enclosures (the counter-view on stiffness and tightness): https://rdphysics.com/2020/09/23/3d-printed-spherical-enclosures/
- The 1973 vented-box paper, part 1 (leakage Q, misalignment charts): https://sdlabo.jp/archives/Vented_Box_Loudspeaker%20Systems_Part_1-4.pdf
- Tutorial on vented alignments (Q_L = 7): https://audiojudgement.com/bass-reflex-alignments-explained/
- FDM dimensional accuracy, service guide: https://www.hubs.com/knowledge-base/dimensional-accuracy-3d-printed-parts/
- FDM tolerances, second service: https://www.protolabs.com/services/3d-printing/fused-deposition-modeling/
- The printer's maker page (build volume, hotend, bed, chamber): https://us.elegoo.com/pages/elegoo-centauri-carbon
- MDF 3/4 in 4 x 8 ft, $59.00: https://www.woodworkerssource.com/plywood-sheet-goods/mdf-34.html
- MDF 3/4 in 49 x 97 in, $76.88: https://www.dunnlumber.com/all-departments/lumber-plywood/plywood-sheetgoods/composite-panels/mdf-3-4-inches-medium-density-fiberboard-49-inches-x-97-inches-formaldehyde-free-mdf34.html
- Baltic birch 18 mm 5 x 5 ft, $118.88, graded "Interior": https://www.dunnlumber.com/russian-birch-18mm-hardwood-plywood-rotary-cut-interior-carb-ii-5-feet-x-5-feet-balticbirch18mm.html
- Baltic birch 3/4 in 13-ply 4 x 8 ft, $91.59: https://www.bairdbrothers.com/b34-Baltic-Birchb-Plywood-13-ply-b48-x-96b-sheet-size-P55184.aspx
- Baltic birch 3/4 in 30 x 30 in, $45.99: https://www.rockler.com/baltic-birch-plywood-choose-thickness
- ASA 1 kg, $24.99: https://shop.polymaker.com/products/asa.js
- PETG 1 kg, $18.99: https://shop.polymaker.com/products/petg.js
- PLA 1 kg, $19.99: https://shop.polymaker.com/products/polylite-pla.js
- PLA 1 kg, $29.99: https://www.prusa3d.com/product/prusament-pla-prusa-galaxy-black-1kg/
- PETG 1 kg, $29.99: https://www.prusa3d.com/product/prusament-petg-jet-black-1kg/
- PLA 1 kg, $15.99: https://us.store.bambulab.com/products/pla-basic-filament
- The owner's inventory records (tool, spool, spool-event and material registers), at the commit of 2026-10-06; private, no public URL
- The program brief, §1 (K22, K67, K72, K88, K89, K90), §5 (the proposals table) and §28-§30: https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md
- The amendment of 2026-10-01, items 3 and 7: https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/AMENDMENT-2026-10-01.md

## What was read

Read 2026-10-06. In this repository: `docs/proposals/P1-embedded-platform.md` and
`docs/proposals/P14-devices-seam.md` (the shape), `docs/conventions.md` (sections 16, 18 and 19),
`tools/conventions/check-provenance.sh`, and the pinned program brief and amendment linked under
Sources. In the owner's inventory repository (a read-only sibling clone): the tool records for
power tools, shop, finishing and measurement, the spool register and its event log, and the sheet,
adhesive and edge-band material records. On the web: every URL under Sources, each a maker's data
sheet, a handbook, a published paper or article, a material guide or a seller's price page. Pages
that refused the fetch and were therefore not used: two home-centre price pages, a forum thread on
printed enclosures, a journal paper on printed enclosure materials, and a model-sharing page for a
filled-wall subwoofer. No GPL source file and no reciprocally licensed hardware design file was
opened; the printed-speaker pages read are articles, not design files. No repository was modified
besides this one.
