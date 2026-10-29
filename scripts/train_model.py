"""Trains the model that turns a screen's text into the page describing it.

The task, precisely: each row of a recorded screen has to be given a kind — a run of
ordinary text, a tool call, a meter, a reasoning line — and then rows of the same kind are
grouped into components. That is the whole of what the page adds over the text, so it is the
whole of what is learned here.

Why a linear model and not something larger: there are on the order of a hundred training
pairs, produced one recording at a time. A network with more parameters than that would
memorise the corpus and report a number that means nothing. A softmax regression over
features chosen from what the screens actually look like is the honest size of model for
this much data, and it can be checked by hand.

The features are deliberately textual and positional — nothing looks at the recording, the
events, or the previous frame. A model that needed those would need the recorder running
beside it, and the point is to convert a screen.

Usage:
    python scripts/train_model.py corpus/pairs.jsonl models/page.json [--epochs 400]

Writes the weights as JSON so that inference can be implemented anywhere — the product runs
it in Rust, and `scripts/eval_model.py` scores it with the same comparison the rig uses.
"""

import argparse
import json
import sys

import numpy as np

# The kinds the page model knows about. Anything not one of these is ordinary text.
KINDS = ["Block", "ToolCall", "Meter", "Thought", "Diff"]

BOX = "╭┌╰└│┃─━"
TOOL_GLYPH = "\u2666"  # ♦


def rows_of(component):
    """The half-open row range a component covers."""
    rows = component.get("rows") or [0, 0]
    return int(rows[0]), int(rows[1])


def label_rows(page):
    """Which kind each row of a page belongs to.

    A row inside a component takes that component's kind; a row no component covers is
    ordinary text, which is what it is on screen.
    """
    labels = {}
    surface = page.get("createSurface", {})
    for component in surface.get("components", []):
        kind = component.get("component", "Block")
        if kind == "Column":
            continue
        first, end = rows_of(component)
        for row in range(first, end):
            # The first component to claim a row keeps it: a later one overlapping it is
            # the reader describing the same cells twice.
            labels.setdefault(row, kind)
    return labels


def features(line, row, row_count):
    """What a row looks like, as numbers a linear model can weigh.

    Every feature is something a person would notice reading the screen: a glyph at the
    start, a number against a limit, a border, a list of keys.
    """
    text = line.rstrip()
    stripped = text.strip()
    leading = len(text) - len(text.lstrip())

    def has(predicate):
        return 1.0 if predicate else 0.0

    digits = sum(character.isdigit() for character in stripped)
    return [
        has(stripped.startswith(TOOL_GLYPH)),                    # a tool line
        has("/" in stripped and digits > 0),                     # "1.2K / 500K"
        has("|" in stripped),                                    #
        has("Ctrl+" in stripped or "Shift+" in stripped),        # a shortcut row
        has("→" in stripped),                                    # a shortcut with a destination
        has(stripped.startswith("/")),                           # a slash command
        has("+ " in stripped or "- " in stripped),               # an edit summary
        has(stripped.endswith(":")),                             # a property label
        has(any(stripped.startswith(character) for character in BOX)),
        has(any(stripped.endswith(character) for character in BOX)),
        has(stripped.startswith(">")),                           # a prompt
        has(stripped == ""),
        digits / max(len(stripped), 1),
        len(stripped) / 120.0,
        leading / 20.0,
        row / max(row_count, 1),                                 # vertical position
        has(stripped[:1].isascii() is False),                    # starts with a symbol
        has(any(word in stripped for word in ("hook", "Edited", "Read", "Ran", "Wrote", "Explored", "Search", "Listed"))),
        1.0,                                                     # bias
    ]


def load_pairs(path):
    """Every pair in a JSON Lines export, as a screen's rows and its page."""
    pairs = []
    with open(path, encoding="utf-8") as source:
        for line in source:
            line = line.strip()
            if not line:
                continue
            pair = json.loads(line)
            page = json.loads(pair["target"])
            rows = pair["input"].split("\n")
            # The recording a pair came from is kept when the export carries it, so a measurement can ask how a
            # model does on *this* application. Before round 112 the export did not write it and this loader could
            # not have kept it; now both do, and `compare_capacity.py` reads it.
            source = pair.get("source", "")
            pairs.append((rows, page, source))
    return pairs


def build_dataset(pairs):
    """Rows, labels and the index of the pair each row came from."""
    samples, labels, groups = [], [], []
    for index, (rows, page, _source) in enumerate(pairs):
        kinds = label_rows(page)
        for row, line in enumerate(rows):
            samples.append(features(line, row, len(rows)))
            labels.append(KINDS.index(kinds.get(row, "Block")))
            groups.append(index)
    sources = [
        (pair[2] if len(pair) > 2 else "")
        for pair in pairs
    ]
    return (
        np.array(samples, dtype=np.float64),
        np.array(labels),
        np.array(groups),
        sources,
    )


def softmax(logits):
    shifted = logits - logits.max(axis=1, keepdims=True)
    exponentiated = np.exp(shifted)
    return exponentiated / exponentiated.sum(axis=1, keepdims=True)


def train(x, y, epochs, learning_rate, l2):
    """Softmax regression by gradient descent, with class weights for the rare kinds.

    The rare kinds matter more than their frequency suggests: a page is judged partly on
    whether it found the meter at all, and meters are a handful of rows in a hundred. Left
    unweighted, the model would learn to call everything ordinary text — which is exactly
    the do-nothing converter this has to beat.
    """
    count, dimensions = x.shape
    classes = len(KINDS)
    weights = np.zeros((dimensions, classes))
    bias = np.zeros(classes)

    totals = np.bincount(y, minlength=classes).astype(np.float64)
    present = totals > 0
    class_weight = np.ones(classes)
    class_weight[present] = count / (present.sum() * totals[present])
    row_weight = class_weight[y]

    targets = np.zeros((count, classes))
    targets[np.arange(count), y] = 1.0

    for _ in range(epochs):
        probabilities = softmax(x @ weights + bias)
        error = (probabilities - targets) * row_weight[:, None]
        weights -= learning_rate * (x.T @ error / count + l2 * weights)
        bias -= learning_rate * error.mean(axis=0)
    return weights, bias


def accuracy(x, y, weights, bias):
    predictions = softmax(x @ weights + bias).argmax(axis=1)
    return float((predictions == y).mean()), predictions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("pairs")
    parser.add_argument("out")
    parser.add_argument("--epochs", type=int, default=400)
    parser.add_argument("--learning-rate", type=float, default=0.5)
    parser.add_argument("--l2", type=float, default=0.001)
    parser.add_argument("--holdout", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=7)
    arguments = parser.parse_args()

    pairs = load_pairs(arguments.pairs)
    if not pairs:
        sys.exit("no pairs to train on")
    x, y, groups, _sources = build_dataset(pairs)

    # Split by pair, not by row: rows of one screen are not independent, and splitting them
    # would let a model that memorised a screen report success on the rest of it.
    rng = np.random.default_rng(arguments.seed)
    order = rng.permutation(len(pairs))
    cut = max(1, int(len(pairs) * (1.0 - arguments.holdout)))
    train_pairs = set(order[:cut].tolist())
    train_rows = np.array([group in train_pairs for group in groups])
    if not train_rows.any() or train_rows.all():
        sys.exit("the split left nothing on one side")

    weights, bias = train(
        x[train_rows], y[train_rows], arguments.epochs, arguments.learning_rate, arguments.l2
    )
    seen, _ = accuracy(x[train_rows], y[train_rows], weights, bias)
    unseen, predictions = accuracy(x[~train_rows], y[~train_rows], weights, bias)

    print(f"pairs: {len(pairs)}  rows: {len(y)}")
    print("rows per kind: " + ", ".join(
        f"{KINDS[index]}={int(total)}" for index, total in enumerate(np.bincount(y, minlength=len(KINDS)))
    ))
    print(f"row accuracy — train {seen:.1%}, held out {unseen:.1%}")
    held_labels = y[~train_rows]
    for index, kind in enumerate(KINDS):
        mask = held_labels == index
        if mask.sum():
            found = (predictions[mask] == index).mean()
            print(f"  held-out {kind:<9} recall {found:.1%} ({int(mask.sum())} rows)")

    model = {
        "kinds": KINDS,
        "features": 19,
        "weights": weights.tolist(),
        "bias": bias.tolist(),
        "trainedOn": len(pairs),
    }
    with open(arguments.out, "w", encoding="utf-8") as sink:
        json.dump(model, sink, indent=1)
    print(f"wrote {arguments.out}")


if __name__ == "__main__":
    main()
