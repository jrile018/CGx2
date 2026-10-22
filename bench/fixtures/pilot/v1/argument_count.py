import sys


def adjust(n: int) -> int:
    return n * 7 + 1


print(adjust(len(sys.argv) - 1))
