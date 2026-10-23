"""Summary of the CSV of PMD CPD: one line a clone, the largest first."""

import os
import sys

TOP = 40


def read(path):
    clones = []
    if not os.path.exists(path):
        return clones
    with open(path) as file:
        next(file, None)
        for row in file:
            cells = row.rstrip("\n").split(",")
            tokens, count = int(cells[0]), int(cells[1])
            places = []
            for i in range(2, 2 + 3 * count, 3):
                line, lines, name = cells[i], int(cells[i + 1]), cells[i + 2]
                places.append((os.path.relpath(name), int(line), lines))
            clones.append((tokens, places))
    return clones


def main():
    clones = [clone for path in sys.argv[1:] for clone in read(path)]
    clones.sort(key=lambda clone: -clone[0] * (len(clone[1]) - 1))
    spare = sum(places[0][2] * (len(places) - 1) for _, places in clones)
    print(f"{len(clones)} clones, about {spare} lines copied")
    for tokens, places in clones[:TOP]:
        where = "  ".join(f"{name}:{line}+{lines}" for name, line, lines in places)
        print(f"{tokens:5} {where}")


main()
