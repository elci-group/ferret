# ferret-run — the running ferret emoji sequence

Six `.xpr` emoji frames of a sable ferret in a bound gallop (side view,
facing right). Displayed in order — `ferret-run-1` through `ferret-run-6`,
looping — they animate as a ferret running.

| Frame | Phase | Content id |
|-------|-------|------------|
| 1 | flight, full extension | `xpr:ferret:run-1@sha256:4bc7d5…` |
| 2 | tipping into the landing, front paws reaching | `xpr:ferret:run-2@sha256:c3153c…` |
| 3 | coiled over planted front paws | `xpr:ferret:run-3@sha256:a80bdb…` |
| 4 | pivoting over hind paws planted forward | `xpr:ferret:run-4@sha256:b7aa52…` |
| 5 | launching off extended hind legs | `xpr:ferret:run-5@sha256:8b2a0e…` |
| 6 | gathering mid-air for the next reach | `xpr:ferret:run-6@sha256:d43310…` |

Full content ids (run `xprc hash ferret-run-N.xpr` for the exact digest).

## Files

- `ferret-run-[1-6].xpr` — source documents (XPR Display Standard 1.0).
  All frames pass `xprc validate` on every display profile
  (XPR-TXS … XPR-FULL).
- `generate.py` — parametric generator. Every frame shares one skeleton
  (body spine curve, head, mask, tail, limb construction); only the pose
  changes, which keeps proportions identical across the loop. Re-run with
  `python3 generate.py`.
- `png/` — 512×512 renders (`XPR-FULL`) plus `run-cycle-sheet.png`
  contact sheet.
- `ferret-run.ttf` — all six frames compiled to a color TTF font; the
  frames live at Supplementary PUA codepoints `U+F0000` … `U+F0005` in
  frame order.
- `animate.sh` — plays the cycle in the terminal (`./animate.sh
  [profile] [delay]`, default `XPR-STD` at 10 fps).

## Tooling

Built with [xpressive](https://github.com/elci-group/xpressive)
(`~/xpressive`), e.g.:

```sh
xprc render ferret-run-1.xpr --profile XPR-FULL -o out.png
xprc preview ferret-run-3.xpr --profile XPR-STD
xprc validate ferret-run-5.xpr
```
