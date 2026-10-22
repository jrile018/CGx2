def affine(x: int, scale: int, shift: int) -> int:
    return x * scale + shift


print(affine(affine(7, 3, 2), 4, 9))
