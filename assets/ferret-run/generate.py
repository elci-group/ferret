#!/usr/bin/env python3
"""Parametric generator for the ferret run-cycle .xpr emoji set.

Emits ferret-run-1..6.xpr — six frames of a mustelid bound gallop that
loop into a continuous run. All frames share one skeleton (sable ferret,
side view, facing right); only the pose changes per frame, which keeps
proportions identical so the loop reads as one animal running.

Regenerate:  python3 generate.py
"""

import json
import math
import os

OUT = os.path.dirname(os.path.abspath(__file__))
CANVAS = 1024

# ---------------------------------------------------------------- palette
BACK = {"r": 112, "g": 76, "b": 45}        # sable back fur
BACK_DARK = {"r": 84, "g": 55, "b": 33}    # far limbs / shading
MASK_BROWN = {"r": 62, "g": 40, "b": 25}   # eye mask, near ear
EAR_DARK = {"r": 48, "g": 32, "b": 20}     # far ear
CREAM = {"r": 246, "g": 236, "b": 214}     # belly, muzzle
INNER_EAR = {"r": 228, "g": 168, "b": 158}
NOSE_PINK = {"r": 222, "g": 126, "b": 140}
EYE_BLACK = {"r": 26, "g": 20, "b": 16}
GLINT = {"r": 255, "g": 255, "b": 255}
FOOT_DARK = {"r": 54, "g": 35, "b": 23}
WHISKER = {"r": 252, "g": 250, "b": 244}
SPEED = {"r": 186, "g": 182, "b": 196, "a": 130}

# ---------------------------------------------------------------- poses
# spine: (tail_base, ctrl1, ctrl2, neck) of the body's centre cubic.
# front_*: (elbow, paw) of the front legs; hind_*: (knee, ankle, paw).
# tail: (ctrl1, ctrl2, tip) from the tail base.
POSES = [
    dict(
        id=1,
        desc=("a ferret fully airborne in extension, front paws reaching "
              "forward and hind legs stretched back, tail streaming"),
        emotion="exhilaration", intensity=88,
        spine=((190, 478), (330, 430), (540, 434), (700, 482)),
        head_tilt=2,
        front_near=((782, 640), (882, 700)), front_far=((750, 658), (830, 716)),
        hind_near=((180, 585), (140, 650), (112, 690)),
        hind_far=((212, 602), (180, 662), (156, 700)),
        tail=((70, 460), (16, 424), (36, 368)),
    ),
    dict(
        id=2,
        desc=("a ferret tipping forward into the landing, front paws "
              "reaching for the ground as the hind legs swing through"),
        emotion="determination", intensity=82,
        spine=((170, 542), (330, 508), (540, 522), (700, 562)),
        head_tilt=3,
        front_near=((732, 642), (770, 802)), front_far=((700, 652), (726, 822)),
        hind_near=((332, 622), (355, 700), (372, 762)),
        hind_far=((300, 640), (322, 712), (335, 778)),
        tail=((95, 502), (58, 450), (76, 400)),
    ),
    dict(
        id=3,
        desc=("a ferret coiled over its planted front paws, spine arched "
              "as the hind legs swing forward beneath the body"),
        emotion="effort", intensity=80,
        spine=((205, 640), (345, 560), (540, 555), (688, 600)),
        head_tilt=-2,
        front_near=((615, 735), (665, 855)), front_far=((585, 738), (610, 858)),
        hind_near=((390, 655), (412, 730), (475, 742)),
        hind_far=((362, 672), (390, 758), (448, 805)),
        tail=((140, 585), (110, 505), (130, 448)),
    ),
    dict(
        id=4,
        desc=("a ferret pivoting over hind paws planted ahead of its "
              "shoulders, rounded back and front legs lifting off"),
        emotion="effort", intensity=80,
        spine=((240, 600), (380, 505), (550, 495), (675, 480)),
        head_tilt=-9,
        front_near=((640, 618), (578, 652)), front_far=((614, 630), (552, 664)),
        hind_near=((435, 655), (495, 762), (548, 852)),
        hind_far=((410, 668), (465, 770), (515, 854)),
        tail=((175, 525), (142, 440), (168, 382)),
    ),
    dict(
        id=5,
        desc=("a ferret launching upward off fully extended hind legs, "
              "body stretched and rising, front legs tucked"),
        emotion="exhilaration", intensity=90,
        spine=((258, 572), (400, 514), (565, 468), (705, 430)),
        head_tilt=-10,
        front_near=((682, 555), (660, 668)), front_far=((652, 562), (628, 670)),
        hind_near=((238, 688), (170, 780), (150, 854)),
        hind_far=((268, 698), (205, 782), (188, 856)),
        tail=((184, 542), (118, 498), (86, 438)),
    ),
    dict(
        id=6,
        desc=("a ferret gathering mid-air, rising, front legs beginning "
              "their next reach as the hind legs trail"),
        emotion="joy", intensity=84,
        spine=((218, 508), (360, 450), (545, 424), (700, 416)),
        head_tilt=-6,
        front_near=((740, 500), (806, 542)), front_far=((712, 514), (768, 564)),
        hind_near=((214, 606), (178, 672), (170, 724)),
        hind_far=((244, 618), (212, 680), (208, 728)),
        tail=((148, 468), (106, 420), (126, 366)),
    ),
]

# ---------------------------------------------------------------- geometry
K = 0.5523  # cubic-bezier circle constant


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def rot(p, deg):
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c)


def cubic(p0, p1, p2, p3, n):
    pts = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        x = mt**3 * p0[0] + 3 * mt**2 * t * p1[0] + 3 * mt * t**2 * p2[0] + t**3 * p3[0]
        y = mt**3 * p0[1] + 3 * mt**2 * t * p1[1] + 3 * mt * t**2 * p2[1] + t**3 * p3[1]
        pts.append((x, y))
    return pts


def offset_chain(pts, halfwidths):
    """Points offset along curve normals by halfwidths; returns both sides."""
    n = len(pts)
    plus, minus = [], []
    for i, (x, y) in enumerate(pts):
        if i == 0:
            dx, dy = pts[1][0] - x, pts[1][1] - y
        elif i == n - 1:
            dx, dy = x - pts[-2][0], y - pts[-2][1]
        else:
            dx, dy = pts[i + 1][0] - pts[i - 1][0], pts[i + 1][1] - pts[i - 1][1]
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length, dx / length
        h = halfwidths[i]
        plus.append((x + nx * h, y + ny * h))
        minus.append((x - nx * h, y - ny * h))
    return plus, minus


# ---------------------------------------------------------------- commands
def mv(x, y):
    return {"op": "move_to", "x": int(round(x)), "y": int(round(y))}


def ln(x, y):
    return {"op": "line_to", "x": int(round(x)), "y": int(round(y))}


def qd(cx, cy, x, y):
    return {"op": "quad_to", "cx": int(round(cx)), "cy": int(round(cy)),
            "x": int(round(x)), "y": int(round(y))}


def cb(c1x, c1y, c2x, c2y, x, y):
    return {"op": "cubic_to", "c1x": int(round(c1x)), "c1y": int(round(c1y)),
            "c2x": int(round(c2x)), "c2y": int(round(c2y)),
            "x": int(round(x)), "y": int(round(y))}


CLOSE = {"op": "close"}


def ri(v):
    return int(round(v))


# ---------------------------------------------------------------- paths
def path(commands, fill=None, stroke=None, feature=None, opacity=None):
    p = {"commands": commands}
    if feature:
        p["feature"] = feature
    if fill:
        p["fill"] = {"color": fill}
    if stroke:
        col, width = stroke
        s = {"color": col, "width": ri(width)}
        p["stroke"] = s
    if opacity is not None and opacity != 255:
        p["opacity"] = opacity
    return p


def ellipse_cmds(cx, cy, rx, ry, deg):
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)

    def w(px, py):
        return (cx + px * c - py * s, cy + px * s + py * c)

    segs = [
        ((rx, 0), (rx, K * ry), (K * rx, ry), (0, ry)),
        ((0, ry), (-K * rx, ry), (-rx, K * ry), (-rx, 0)),
        ((-rx, 0), (-rx, -K * ry), (-K * rx, -ry), (0, -ry)),
        ((0, -ry), (K * rx, -ry), (rx, -K * ry), (rx, 0)),
    ]
    cmds = []
    p0, _, _, _ = segs[0]
    start = w(*p0)
    cmds.append(mv(*start))
    for p_from, c1, c2, p_to in segs:
        f = w(*p_from)
        a1, a2 = w(*c1), w(*c2)
        t = w(*p_to)
        cmds.append(cb(a1[0], a1[1], a2[0], a2[1], t[0], t[1]))
    cmds.append(CLOSE)
    return cmds


def circle(cx, cy, r, fill):
    return path(ellipse_cmds(cx, cy, r, r, 0), fill=fill)


def rounded_poly(verts, cuts, fill):
    n = len(verts)

    def entry(i):
        prev = verts[(i - 1) % n]
        return lerp(verts[i], prev, cuts[i])

    def exit_(i):
        nxt = verts[(i + 1) % n]
        return lerp(verts[i], nxt, cuts[i])

    cmds = [mv(*entry(0))]
    for i in range(n):
        v = verts[i]
        e = exit_(i)
        cmds.append(qd(v[0], v[1], e[0], e[1]))
        nxt_entry = entry((i + 1) % n)
        cmds.append(ln(*nxt_entry))
    cmds.append(CLOSE)
    return path(cmds, fill=fill)


def capsule(p_from, p_to, width, color):
    cmds = [mv(*p_from), ln(*p_to)]
    return path(cmds, stroke=(color, width))


def poly_stroke(points, width, color):
    cmds = [mv(*points[0])]
    for p in points[1:]:
        cmds.append(ln(*p))
    return path(cmds, stroke=(color, width))


# ---------------------------------------------------------------- ferret
H_BODY = 92          # body half-width at the waist
HEAD_RX, HEAD_RY = 152, 132
LEG_W = 52
TAIL_W0, TAIL_W1 = 38, 5


def body_halfwidth(t, h=H_BODY):
    return h * (0.45 + 0.55 * math.sin(math.pi * t))


def spine_t(spine_pts, t):
    """Point on the sampled centre line at parameter t in [0, 1]."""
    n = len(spine_pts) - 1
    i = min(int(t * n), n - 1)
    return lerp(spine_pts[i], spine_pts[i + 1], t * n - i)


def build_frame(pose):
    p0, p1, p2, p3 = pose["spine"]
    samples = cubic(p0, p1, p2, p3, 20)
    hw = [body_halfwidth(i / 20) for i in range(21)]

    # -- speed lines behind the runner, kept inside the canvas
    rear_x = p0[0]

    def sx(x):
        return max(30, min(994, x))

    speed_ys = (p0[1] - 42, p0[1] + 62, p0[1] + 152)
    speed = [
        capsule((sx(rear_x - 236), speed_ys[0]), (sx(rear_x - 116), speed_ys[0]), 16, SPEED),
        capsule((sx(rear_x - 200), speed_ys[1]), (sx(rear_x - 84), speed_ys[1]), 16, SPEED),
        capsule((sx(rear_x - 228), speed_ys[2]), (sx(rear_x - 112), speed_ys[2]), 16, SPEED),
    ]

    # -- soft ground shadow (stronger when paws are planted)
    planted = pose["id"] in (2, 3, 4, 5)
    scx = (p0[0] + p3[0]) / 2 + 40
    shadow = path(
        ellipse_cmds(scx, 935, 210 if planted else 150, 26, 0),
        fill={"r": 120, "g": 116, "b": 128, "a": 62 if planted else 44},
    )

    # -- body outline
    plus, minus = offset_chain(samples, hw)
    cmds = [mv(*plus[0])]
    for pt in plus[1:]:
        cmds.append(ln(*pt))
    for pt in reversed(minus[:-1]):
        cmds.append(ln(*pt))
    cmds.append(CLOSE)
    body = path(cmds, fill=BACK, feature="body")

    # -- cream belly, a capsule riding just under the spine
    belly_pts = []
    for t in (0.16, 0.5, 0.84):
        c = spine_t(samples, t)
        idx = int(t * 20)
        x, y = samples[idx]
        dx, dy = samples[idx + 1][0] - samples[idx - 1][0], samples[idx + 1][1] - samples[idx - 1][1]
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length, dx / length
        if ny < 0:
            nx, ny = -nx, -ny  # normal pointing down (screen)
        belly_pts.append((c[0] + nx * hw[idx] * 0.10, c[1] + ny * hw[idx] * 0.10))
    belly = poly_stroke(belly_pts, 1.35 * body_halfwidth(0.5), CREAM)

    # -- tail
    t1, t2, tip = pose["tail"]
    tail_pts = cubic(p0, t1, t2, tip, 10)
    tail_hw = [TAIL_W0 + (TAIL_W1 - TAIL_W0) * (i / 10) for i in range(11)]
    tplus, tminus = offset_chain(tail_pts, tail_hw)
    cmds = [mv(*tplus[0])]
    for pt in tplus[1:]:
        cmds.append(ln(*pt))
    cmds.append(ln(*tminus[-1]))
    for pt in reversed(tminus[:-1]):
        cmds.append(ln(*pt))
    cmds.append(CLOSE)
    tail = path(cmds, fill=BACK)

    # -- legs
    shoulder = spine_t(samples, 0.82)
    hip = spine_t(samples, 0.18)

    def hind_leg(knee, ankle, paw, color, w):
        ang = math.degrees(math.atan2(knee[1] - hip[1], knee[0] - hip[0]))
        ctr = lerp(hip, knee, 0.35)
        thigh = path(ellipse_cmds(ctr[0], ctr[1], 96, 74, ang), fill=color)
        shank = poly_stroke([knee, ankle, paw], w, color)
        return [thigh, shank]

    def front_leg(elbow, paw, color, w):
        ang = math.degrees(math.atan2(elbow[1] - shoulder[1], elbow[0] - shoulder[0]))
        ctr = lerp(shoulder, elbow, 0.30)
        arm = path(ellipse_cmds(ctr[0], ctr[1], 78, 60, ang), fill=color)
        limb = poly_stroke([shoulder, elbow, paw], w, color)
        return [arm, limb]

    far_front = front_leg(*pose["front_far"], BACK_DARK, LEG_W - 6)
    far_hind = hind_leg(*pose["hind_far"], BACK_DARK, LEG_W - 6)
    far_feet = [circle(*pose["front_far"][1], 30, FOOT_DARK), circle(*pose["hind_far"][2], 30, FOOT_DARK)]

    near_hind = hind_leg(*pose["hind_near"], BACK, LEG_W + 2)
    near_front = front_leg(*pose["front_near"], BACK, LEG_W + 4)
    near_feet = [circle(*pose["front_near"][1], 32, FOOT_DARK), circle(*pose["hind_near"][2], 32, FOOT_DARK)]

    # -- head
    direction = (p3[0] - p2[0], p3[1] - p2[1])
    dl = math.hypot(*direction) or 1.0
    direction = (direction[0] / dl, direction[1] / dl)
    hc = (p3[0] + direction[0] * 62, p3[1] + direction[1] * 62)
    hang = math.degrees(math.atan2(direction[1], direction[0])) + pose["head_tilt"]

    def loc(px, py):
        x, y = rot((px, py), hang)
        return (hc[0] + x, hc[1] + y)

    head = path(ellipse_cmds(hc[0], hc[1], HEAD_RX, HEAD_RY, hang), fill=BACK, feature="head")
    muzzle = capsule(loc(26, 20), loc(148, 52), 102, CREAM)
    mask = capsule(loc(-92, 4), loc(102, 28), 104, MASK_BROWN)
    mask["feature"] = "mask"

    def loc3(v):
        return [loc(*p) for p in v]

    far_ear = rounded_poly(loc3([(-120, -100), (-44, -106), (-90, -212)]), [0.30, 0.30, 0.34], EAR_DARK)
    near_ear = rounded_poly(loc3([(-42, -106), (46, -110), (8, -238)]), [0.28, 0.28, 0.34], MASK_BROWN)
    inner_ear = rounded_poly(loc3([(-18, -122), (26, -124), (6, -196)]), [0.34, 0.34, 0.38], INNER_EAR)
    eye = circle(*loc(52, -6), 34, EYE_BLACK)
    glint = circle(*loc(64, -19), 12, GLINT)
    nose = circle(*loc(150, 56), 25, NOSE_PINK)
    whiskers = [
        capsule(loc(112, 50), loc(182, 28), 7, WHISKER),
        capsule(loc(116, 56), loc(188, 58), 7, WHISKER),
        capsule(loc(112, 62), loc(180, 88), 7, WHISKER),
    ]

    doc = {
        "header": {"spec_version": "0.1.0"},
        "identity": {"namespace": "ferret", "asset_id": f"run-{pose['id']}"},
        "metadata": {
            "name": f"ferret-run-{pose['id']}",
            "aliases": [f"ferret running {pose['id']}", f"ferret run frame {pose['id']}"],
            "tags": ["ferret", "run", "animal", "sequence", "bound-gallop"],
        },
        "canvas": {"size": CANVAS},
        "layers": [
            {"name": "ground", "paths": [shadow]},
            {"name": "speed", "paths": speed},
            {"name": "far-limbs", "paths": [*far_front, *far_hind, *far_feet]},
            {"name": "tail", "paths": [tail]},
            {"name": "body", "paths": [body, belly]},
            {"name": "near-limbs", "paths": [*near_hind, *near_front, *near_feet]},
            {"name": "head", "paths": [
                far_ear, head, muzzle, mask, near_ear, inner_ear,
                eye, glint, nose, *whiskers,
            ]},
        ],
        "semantics": {
            "description": pose["desc"],
            "emotion": pose["emotion"],
            "intensity": pose["intensity"],
        },
        "provenance": {
            "generator": "ferret-run-parametric/1.0",
            "prompt": ("six-frame mustelid bound-gallop run cycle, side view "
                       "facing right, sable ferret with cream belly and eye mask"),
        },
    }
    return doc


def main():
    for pose in POSES:
        doc = build_frame(pose)
        name = f"ferret-run-{pose['id']}.xpr"
        with open(os.path.join(OUT, name), "w") as f:
            json.dump(doc, f, indent=2)
            f.write("\n")
        print(f"wrote {name}")


if __name__ == "__main__":
    main()
