# Drawing a Gaming Mouse Without Its Vendor Artwork — The Diagram Workflow

> How to produce the `DeviceSpec::diagram` for a new device: find the model's official
> schematic, use it as a *positional reference*, and express the mouse as ~30 lines of
> original shape data. That one definition then drives the settings window (GDI+), the
> generated `docs/assets/<device>.svg`, and the drift test that keeps them in lockstep.
>
> Companion to [`CRACKING-MICE-GUIDE.md`](../CRACKING-MICE-GUIDE.md) (which gets the
> *protocol*; this gets the *picture*). Worked examples throughout are the two shipped
> devices — the **DeathAdder Elite** and the **DeathAdder V3 (wired)** — whose diagrams
> live in [`crates/razer-proto/src/lib.rs`](../crates/razer-proto/src/lib.rs).

---

## 0. The one mental model that makes this tractable

A diagram here is **data, not artwork**. You are not drawing a mouse; you are stating
facts with coordinates: *there is a button at this position, it maps to this config key,
it emits this vendor code*. The DSL
([`crates/razer-proto/src/diagram.rs`](../crates/razer-proto/src/diagram.rs)) has five
primitives and seven styling roles, and everything else — colors, stroke weights, font
sizes, dark/light theming — is decided by the renderers. That is why:

- adding a diagram needs **no artwork tools** — you edit Rust source and look at output;
- the settings window and the docs **cannot drift** — both render the same data, and a
  test regenerates the SVG and fails if the committed file differs;
- the legal position stays clean — coordinates you chose are yours.

One definition, three consumers:

| surface | renderer | where |
|---|---|---|
| settings window pane | GDI+ (anti-aliased) + GDI ClearType text | `crates/platform/src/diagram.rs` |
| `docs/assets/<device>.svg` | `Diagram::to_svg()` emitter | `crates/razer-proto/src/diagram.rs` |
| CI drift check | byte-compares the committed SVG against the emitter | `supported_devices_doc_matches_table` |

---

## 1. Before You Draw: Think Like a Draftsman

Do not start placing Bezier curves immediately.

Instead, reconstruct the geometry of the physical object.

Imagine you are explaining the mouse to another engineer over the phone.

Before writing any code, determine:

- the overall silhouette
- the centerline
- the major asymmetries
- every physical control
- where the shell is widest
- where it narrows
- where the thumb naturally rests

Only once those facts are understood should you encode them into the diagram.

You are not tracing pixels.

You are reconstructing the designer's geometric intent.

The SVG (or Rust DSL) is simply the language used to describe that geometry.

**You are reconstructing the physical object, not reproducing the image.**

---

## 2. Construction Order

Always solve the diagram in roughly this order:

1. Understand the silhouette.
2. Identify asymmetries.
3. Locate the controls.
4. Draw one body path.
5. Add seams.
6. Add buttons.
7. Add leader lines.
8. Add captions.
9. Perform a geometry sanity check.

Do not jump between these steps.

---

## 3. Getting the reference schematic

Razer publishes a labeled top-down diagram for nearly every device — the "device layout"
page from the master guide. That is your positional reference.

The concrete path:

1. Go to **mysupport.razer.com** and search **"`<model>` support"** (e.g. "DeathAdder V3
   support"), or web-search the same phrase — the support page usually outranks the
   store page.
2. Open the model's support page and **verify the model number** (the `RZ01-…` code on
   the mouse's underside label or its box) matches *your* device. Razer reuses names
   relentlessly: *DeathAdder V3*, *V3 Pro*, *V3 HyperSpeed* are **three different
   devices** with different PIDs, shapes, and button sets. The support page lists the
   RZ number — check it, not the marketing name.
   - Worked example: the V3 (wired, `RZ01-04640`) page is
     <https://mysupport.razer.com/app/answers/detail/a_id/6124/>.
3. On the support page, look under **Specifications** or the **master guide** link for
   the labeled device diagram — a PNG with callout letters (A, B, C…) naming every
   control.
   - Worked example (Elite): <https://dl.razerzone.com/src/aag/2043-2-en-v2.png> —
     A/B = left/right click, C = wheel, D/E = the two DPI buttons, F/G = thumb buttons.
   - Worked example (V3): <https://dl.razerzone.com/src2/6128/6128-2-en-v1.png> —
     A = cable, B/C = left/right, D = wheel, E/F = thumb buttons (side view), and the
     underside view showing the DPI button (I) next to the sensor.
4. Link that URL in your device's section of
   [`SUPPORTED-DEVICES.md`](SUPPORTED-DEVICES.md) so the next person can verify your positions.

**If there is no schematic** (rare, but it happens for very new or very old models):
use the physical device in your hand. Photograph it top-down yourself, or just measure
by eye — you need maybe ten facts (roughly where the wheel sits, where the side buttons
start and end, how far down the button plates reach, where the shell is widest). The
diagram is a schematic, not a portrait; ±5 canvas units of error is invisible.

---

## 4. The DSL, taught

A diagram is a `Diagram { width, height, shapes: &[Shape] }` const on the
`DeviceSpec`. Coordinates are **integers** in design units (1 unit = 1 px at 96 dpi
before any scaling), **top-down view, cable at the top**. Both shipped devices use a
780-wide canvas with the mouse occupying roughly x 110–330 and y 28–392, captions in a
right-hand column starting at x 402, thumb captions in a left column ending by x ~106.
Copy those conventions; the renderers don't require them, but consistency makes every
device's diagram feel like the same hand drew it.

`width`/`height` are only a hint — both renderers compute the real content bounds
(including estimated text widths) and size themselves from that, which is why captions
can't clip (see §5's pitfalls for the history behind this).

### The five shapes

| variant | what it's for |
|---|---|
| `Path { start, curves, closed }` | anything curved: the body silhouette, seams, the logo's S-mark. `curves` is a list of cubic-bezier segments `((c1x,c1y),(c2x,c2y),(endx,endy))` chained from `start`. |
| `RoundRect { x, y, w, h, r }` | buttons and the wheel — every physical control that is roughly a rounded rectangle from above. |
| `Circle { cx, cy, r }` | round markers (the Elite's logo LED). |
| `Polyline { points }` | straight line runs: the button split, and every leader line from a feature to its caption. |
| `Text { at, anchor, text }` | one line of text; `at` is the **baseline** point (SVG semantics), `anchor` is `Start`/`Middle`/`End` horizontal alignment. |

### The seven roles

Roles carry *all* styling — pick the role for what the element **is**, never for how
you want it to look:

| role | use it for | renders as |
|---|---|---|
| `Body` | the outer silhouette | strong 1.6 stroke, foreground color |
| `Detail` | interior line work: button split, seams, an *unlit* wheel | thinner 1.1 stroke |
| `Lead` | leader lines from feature to caption | 1.0 stroke, faded |
| `Label` | the main caption line (the config-key fact) | 13 px foreground text |
| `Note` | context lines, footnotes, the left/right click tags | 11 px dim text |
| `RgbZone` | lighting zones — shapes *and* their zone captions | green accent |
| `Button` | remappable buttons — shapes *and* their code captions | blue accent |

So: the Elite's wheel is `RgbZone` (it's a lighting zone) while the V3's wheel is
`Detail` (same physical object, no LED). The color difference between the two diagrams
is carried entirely by that one word.

The platform layer never sees Razer vocabulary: the daemon maps `RgbZone`/`Button` to
generic `AccentA`/`AccentB` slots, and the window picks actual colors from the system
palette. You never specify a color anywhere.

### An alternative authoring path: SVG first

You don't have to author DSL coordinates directly. A maintainer-validated alternative:
draft the schematic as a plain SVG first — any capable model can do this (GPT-5.5-instant
worked well) — following §2's construction order and keeping the line work original, then
convert the SVG's paths into the DSL's shapes (`C` cubic beziers map 1-to-1 to `Path`
curves, `<rect>`/`<polyline>` to `RoundRect`/`Polyline`). Iterating on a shape in an SVG
viewer is often much easier than reasoning about raw coordinates; §5.0 has the scaling
formulas for translating the finished SVG onto the standard canvas.

---

## 5. How to actually draw

The method used for both shipped diagrams, as a repeatable recipe. Total honest effort:
about an hour the first time, most of it in the verification loop.

### 5.0 Tip: Prototype in SVG first

Prototype in SVG because it gives immediate visual feedback. The SVG is disposable. The geometry is the asset.
1. **Draw or prototype in SVG first:** Create a simple `<svg viewBox="0 0 500 700">` file.
   * *Drafting with AI:* I have had the most success using **ChatGPT 5.5 Instant** specifically to read reference schematics and write the initial SVG template coordinates.
2. **Translate to Rust coordinates:** Once the shape looks correct, scale and shift the SVG coordinates to fit our `780x430` grid (typically centered at `x=229`, starting at `y=30`). You can use a simple scaling formula, for example:
   * `X_new = ((X_svg - X_center) * scale_factor + 229).round() as i32`
   * `Y_new = ((Y_svg - Y_start) * scale_factor + 30).round() as i32`
3. **Map 1-to-1 to the Rust DSL:** Since the Rust diagram DSL maps directly to standard SVG commands (e.g., `Shape::Path` curves map 1-to-1 to SVG's `C` cubic Bezier curves, and `Polyline`/`RoundRect` to `<polyline>`/`<rect>`), you can drop the scaled coordinates straight into the device spec.

### 5.1 The silhouette is the identity of the mouse

Most of the diagram can be slightly wrong without anyone noticing. The silhouette cannot.
People recognize gaming mice almost entirely from:

- thumb flare
- shoulder width
- rear taper
- waist shape
- tail

Do not average these features into a generic mouse. Instead identify what makes this model recognizable. If the body alone (without buttons or labels) is recognizable, the rest of the drawing becomes straightforward.

Write one closed `Body` path, starting at the nose center (or slot corner) and going
counterclockwise (down the left side first). Every Bezier curve should correspond to a physical feature. A curve is not "segment 3." It is:

- left front shoulder
- thumb flare
- rear taper
- tail
- right waist

Think in physical features, not mathematical primitives. Comment each segment:

```rust
Shape::Path { role: Role::Body, start: (228, 30), closed: true, curves: &[
    ((180, 28), (140, 50), (130, 95)),    // left front shoulder
    ((128, 125), (138, 155), (144, 185)), // left waist scoop
    ((148, 205), (130, 230), (128, 255)), // left thumb flare/hip
    ((126, 290), (155, 345), (195, 375)), // lower left toward tail
    ((212, 388), (244, 388), (261, 375)), // rounded tail
    ((301, 345), (322, 290), (320, 255)), // lower right/hip
    ((318, 230), (300, 205), (304, 185)), // right side (pinky waist scoop)
    ((308, 155), (320, 125), (326, 95)),  // right upper side
    ((322, 50), (276, 28), (228, 30)),    // right front shoulder
]},
```

Place endpoints at physical landmarks. Use the control points only to describe the transition between those landmarks. If you find yourself adjusting control points to change the overall shape, your landmarks are probably wrong.

Small proportional errors are acceptable. Pixel-perfect accuracy is not the goal. Recognizability is.

### 5.2 Interior details

The button split (a `Polyline` down the centerline, **broken around the wheel** — two
segments, not one line through it), the button/palm seam (a shallow `Detail` path),
and small `Note` texts "left" / "right" inside the two click plates.

### 5.3 The controls

One `RoundRect` (or custom `Path` for curved items) per physical control, roled by what it is (`RgbZone` wheel, `Button` DPI/thumb buttons, `Detail` unlit wheel). Two rules learned the hard way:

- **Side buttons must protrude from the actual body edge.** Work out the silhouette's
  x at the button's height and overlap it; the V3's first draft reused the Elite's
  x-positions and its thumb buttons floated 25 units clear of the (narrower) body.
- Proportions come from the reference: the Elite's wheel slot is long (h 56 vs the
  V3's 52 but starting much higher), the DPI buttons are a tight center strip directly
  behind it.

### 5.4 Leader lines and the caption columns

Every feature gets a `Lead` polyline running to a fixed caption column — **right column
for features, left column for the thumb buttons** (their captions read toward the
device, `anchor: End`). Keep leads horizontal where possible; when two features are too
close vertically (the Elite's stacked DPI buttons), give the lower one a dog-leg:

```rust
Shape::Polyline { role: Role::Lead, points: &[(242, 163), (340, 163), (372, 186), (396, 186)] },
```

### 5.5 Captions: Label states the fact, Note adds the context

Each feature gets a two-line caption block, baseline 3 units above the lead's y, second
line +16:

- **`Label` line — the config-key fact**: `dpi_up — default: copy`,
  `scroll wheel — middle click`, `"forward"`.
- **Accent/`Note` line — the context**: vendor code, zone id, and *positional
  description* so a human can find the physical button: `code 0x20 · front button,
  nearer the wheel`, `RGB zone 0x01`, `XBUTTON2`.

Annotation content rules:

- Name the **config key** exactly (`dpi_up`, not "DPI+ button").
- Include the **vendor code / zone id** where one exists — it ties the picture to the
  protocol doc.
- State the **default** binding for remappable buttons.
- Describe **position in words** when two buttons could be confused (front/rear,
  nearer the wheel).
- **Document controls that exist but can't be remapped.** Silence reads as an
  omission. The V3 is the worked example — it has a DPI button, just not where users
  of the Elite expect and not one we can touch:

  ```rust
  Shape::Text { role: Role::Note, at: (402, 160), .., text: "no wheel DPI buttons — the DPI button is on" },
  Shape::Text { role: Role::Note, at: (402, 176), .., text: "the underside; it cycles onboard stages in" },
  Shape::Text { role: Role::Note, at: (402, 192), .., text: "firmware and can't be remapped or listened to" },
  ```
- **Put the hook disclaimer at the point of decision**: the thumb-hook cost note is a
  short wrapped `Note` block directly under the thumb callouts — exactly where the
  settings window mounts the remap dropdowns — not a distant footnote. Devices with
  vendor-code buttons additionally keep the hook-free driver-mode line as a centered
  footnote (`Note`, `anchor: Middle`, under everything).
- The caption text you write on a `Callout` appears **only in the generated SVG**: in
  the settings window each callout mounts a dropdown whose index-0 entry names its own
  button (`← Back (default)`, `Front DPI — unbound`), so the captions are suppressed
  there. Jargon like `XBUTTON1` is fine in the SVG/docs; it never reaches the window.

### 5.6 Pitfalls we actually hit (check for all of them)

- **Caption clipping**: the very first hand-made SVG had a footnote running past the
  viewBox. Both renderers now derive bounds from content *including estimated text
  width*, so this is solved structurally — but text width is an **estimate** (0.62 em
  per char). If you write an unusually wide caption (all-caps, lots of `W`s), look at
  the rendered edge.
- **Captions grazing the outline**: the "left"/"right" tags originally sat at a height
  where the shoulder curve cut through them. If a caption sits *inside* the body, put
  it somewhere the outline demonstrably isn't — and re-check after any silhouette edit.
- **Leader lines striking text**: an early draft ran the wheel's lead straight through
  the "right" caption. Leads and in-body text must not share a y at the same x-range.
- **Two fonts, two widths**: the window measures text in Segoe UI, the SVG estimates a
  monospace stack. Each surface measures for itself, so nothing clips — but visual
  spacing can differ slightly between them. Judge each surface by its own screenshot.

### 5.7 Geometry sanity check

Hide every layer except the body. Does it still look like the mouse?
* **If yes:** Continue.
* **If not:** The silhouette is wrong. Fix it before touching anything else.

---

## 6. Drawing Guidelines & Legals

To ensure our diagrams look great and stay legally clear:

* **Mouse Silhouette & Shape:** Razer does not own the outline, contours, or physical shape of the mouse. Functional shapes are not copyrightable. We should make the mouse silhouette as accurate to the physical device as possible. Use the reference images or physical measurements to get the proportions, waist scoop, and flares correct.
* **No Trademarked Logos:** The Razer triple-snake logo is trademarked and is not included in our diagrams. For lighting zones (like the Elite's palm LED), we represent the zone as an abstract circle with a simple "S" curve and the label `RGB zone 0x04`.
* **Original Vector Source:** While the physical shapes are free to replicate, we express them via our own original coordinate paths in the Rust DSL rather than direct vector copies of Razer's marketing assets.

---

## 7. The verification loop

Iterate against rendered output — coordinates in your head lie to you.

1. **Emit the SVG**:
   ```
   cargo test -p razer-proto -- --ignored regenerate_diagram_svgs
   ```
   writes `docs/assets/<slug>.svg` (slug = lowercased name, non-alphanumerics → `-`).
2. **Look at it** — open the SVG in a browser. Check both themes (the file responds to
   `prefers-color-scheme`; toggle the OS setting or use devtools emulation).
3. **Run the drift test** — it should now pass, proving asset == data:
   ```
   cargo test -p razer-proto supported_devices_doc_matches_table
   ```
   (It also requires your device's row in `SUPPORTED-DEVICES.md` and the
   `assets/<slug>.svg` embed to exist.)
4. **See the GDI+ rendering** — the settings window is the surface users actually see:
   ```
   cargo run -p platform --example settings_smoke -- --hold 30            # full-featured layout
   cargo run -p platform --example settings_smoke -- --minimal --hold 30  # no-RGB/no-DPI-buttons layout
   ```
   (The smoke example uses built-in sample geometry; to see *your* device's real data,
   run the daemon and open Settings from the tray — or temporarily paste your shapes
   into the example.)
5. **Screenshot and look.** That is the acceptance bar — not "it compiles", not "the
   test is green": a human looked at the rendered diagram and found no overlaps, no
   clipped text, no floating buttons, and a silhouette that reads as *this* mouse.
   Fix, regenerate, look again.

---

## 8. Checklist

### Geometry Quality

- [ ] Silhouette is instantly recognizable.
- [ ] Scroll wheel is centered.
- [ ] Controls are properly attached.
- [ ] Side buttons cleanly overlap the shell.
- [ ] Seams terminate naturally at the body boundary.
- [ ] No floating features or gaps.
- [ ] Ergo asymmetry is preserved.
- [ ] Recognized as the specific model without needing labels.

### Code Quality (Do/Don't)

Do:

- [ ] Found the model's support page on mysupport.razer.com and **verified the RZ
      model number** against the physical device (beware V3 / V3 Pro / HyperSpeed
      near-namesakes).
- [ ] Used the official schematic or physical measurements for positions and proportions.
- [ ] Wrote the silhouette as your own beziers, capturing the model's physical shape
      (flares, scoop, tail, cutouts) — commented per segment.
- [ ] Roled every element by what it **is** (`RgbZone`/`Button`/`Detail`…), never by
      color.
- [ ] Captioned every control: config key, vendor code / zone id, default, and a
      description — including controls that exist but cannot be remapped.
- [ ] Regenerated the SVG, ran the drift test, ran `settings_smoke`, and verified the
      rendering on both themes.

Don't:

- [ ] Download Razer's PNG files directly into the repository — link them instead to keep the repo clean.
- [ ] Draw the trademarked triple-snake logo — represent lighting zones as abstract circle/S-curve markers.
- [ ] Hardcode colors or styling in the diagram data — use roles so the renderers handle theming.
