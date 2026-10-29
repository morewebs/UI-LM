"""One split, two models: the shipped linear softmax and a small hidden-layer network.

Run from the repository root with the pairs file in `PAIRS`. Both models see the same features, the same rows and
the same grouped holdout — whole screens are held out, never single rows, because a row's label depends on the
rows around it and a row-wise split would flatter both.

This exists because the model card says the model is at its ceiling on this data, and that claim was made by
measuring three ways that all used the same hundred weights. The question here is whether the ceiling is the
*data* or the *model*.
"""

import importlib.util
import os
import sys

import numpy as np

SPEC = importlib.util.spec_from_file_location("trainer", "scripts/train_model.py")
TRAINER = importlib.util.module_from_spec(SPEC)
sys.argv = ["train_model.py", "--help"]
try:
    SPEC.loader.exec_module(TRAINER)
except SystemExit:
    pass

X, LABELS, GROUPS, SOURCES = TRAINER.build_dataset(TRAINER.load_pairs(os.environ["PAIRS"]))
CLASSES = int(LABELS.max()) + 1
ONEHOT = np.eye(CLASSES)[LABELS]


def split(seed=0, holdout=0.2):
    rng = np.random.default_rng(seed)
    ids = np.unique(GROUPS)
    rng.shuffle(ids)
    cut = int(len(ids) * (1 - holdout))
    train_ids = set(ids[:cut].tolist())
    train = np.array([g in train_ids for g in GROUPS])
    globals()["train_ids"] = train_ids
    return X[train], ONEHOT[train], X[~train], LABELS[~train]


def accuracy(predicted, truth):
    return float((predicted == truth).mean())


def softmax_rows(logits):
    shifted = logits - logits.max(axis=1, keepdims=True)
    p = np.exp(shifted)
    return p / p.sum(axis=1, keepdims=True)


def linear(x_train, y_train, x_test, epochs=600, rate=0.05, l2=0.001):
    """The shipped model: one weight per feature per class, no hidden layer."""
    weights = np.zeros((x_train.shape[1], CLASSES))
    bias = np.zeros(CLASSES)
    for _ in range(epochs):
        probs = softmax_rows(x_train @ weights + bias)
        gradient = (probs - y_train) / len(x_train)
        weights -= rate * (x_train.T @ gradient + l2 * weights)
        bias -= rate * gradient.sum(axis=0)
    return accuracy(np.argmax(x_test @ weights + bias, axis=1), None)


def hidden(x_train, y_train, x_test, width=64, epochs=600, rate=0.05):
    """One hidden layer, ~19x the parameters."""
    rng = np.random.default_rng(0)
    w1 = rng.normal(0, 1 / np.sqrt(x_train.shape[1]), (x_train.shape[1], width))
    b1 = np.zeros(width)
    w2 = rng.normal(0, 1 / np.sqrt(width), (width, CLASSES))
    b2 = np.zeros(CLASSES)
    for _ in range(epochs):
        layer = np.maximum(x_train @ w1 + b1, 0)
        probs = softmax_rows(layer @ w2 + b2)
        gradient = (probs - y_train) / len(x_train)
        g_w2 = layer.T @ gradient
        g_b2 = gradient.sum(axis=0)
        g_hidden = (gradient @ w2.T) * (layer > 0)
        w1 -= rate * (x_train.T @ g_hidden)
        b1 -= rate * g_hidden.sum(axis=0)
        w2 -= rate * g_w2
        b2 -= rate * g_b2
    return np.argmax(np.maximum(x_test @ w1 + b1, 0) @ w2 + b2, axis=1)


def main():
    x_train, y_train, x_test, truth = split()
    print(f"rows {len(x_train)} train / {len(x_test)} held out, {X.shape[1]} features, {CLASSES} classes")

    weights = np.zeros((x_train.shape[1], CLASSES))
    bias = np.zeros(CLASSES)
    for _ in range(600):
        probs = softmax_rows(x_train @ weights + bias)
        gradient = (probs - y_train) / len(x_train)
        weights -= 0.05 * (x_train.T @ gradient + 0.001 * weights)
        bias -= 0.05 * gradient.sum(axis=0)
    shipped = accuracy(np.argmax(x_test @ weights + bias, axis=1), truth)
    print(f"linear, 100 parameters:   {shipped:.3f}")

    width = 64
    parameters = X.shape[1] * width + width + width * CLASSES + CLASSES
    print(f"hidden, {parameters} parameters:  {accuracy(hidden(x_train, y_train, x_test), truth):.3f}")

    # Split the same held-out rows by the recording they came from, which is the only way to say how much of a
    # model's error belongs to an application it has never been trained on.
    board = np.argmax(x_test @ weights + bias, axis=1)
    held_pairs = np.flatnonzero(np.array([g not in train_ids for g in GROUPS]))
    by_pair = np.zeros(len(SOURCES), dtype=bool)
    by_pair[: len(SOURCES)] = True
    is_agent = np.array(["agent" in (s or "") for s in SOURCES]) if SOURCES else np.zeros(0, dtype=bool)
    row_agent = is_agent[GROUPS[held_pairs]]
    for name, mask in (("from a second agent", row_agent), ("from the trained application", ~row_agent)):
        if mask.sum():
            print(f"  {name:30s} {int(mask.sum()):5d} rows  accuracy {accuracy(board[mask], truth[mask]):.3f}")


if __name__ == "__main__":
    main()
