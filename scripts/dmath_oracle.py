#!/usr/bin/env python3
"""Independent high-precision oracle for specula_core::dmath (f32 functions).

Reads lines `name arg_bits... result_bits` (hex f32 bit patterns) and checks each result against
the correctly rounded (round to nearest, ties to even) f32 value computed with 120-digit decimal
arithmetic and exact rationals for the final rounding. Lines come from the exhaustive scan:

    cargo test -p specula-core --release dmath::tests::exhaustive -- --ignored --nocapture
    python scripts/dmath_oracle.py target/dmath-hard.txt

or from any list written by hand, e.g. `pow 0x45800800 0x40000000 0x4b800000`.
With `--print`, results are ignored and the correct bits are printed (to pin test cases).

Standard library only. Output is ASCII. Exit status 1 if any result is wrong.
"""
import struct
import sys
from decimal import Decimal, getcontext
from fractions import Fraction

getcontext().prec = 120
EPS = Decimal(10) ** -118


def f32(bits):
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def compute_pi():
    getcontext().prec += 10

    def atan_inv(n):
        x = Decimal(1) / n
        x2, term, total, k = x * x, x, x, 1
        while abs(term) > EPS * EPS:
            term *= -x2
            total += term / (2 * k + 1)
            k += 1
        return total

    pi = 16 * atan_inv(5) - 4 * atan_inv(239)
    getcontext().prec -= 10
    return +pi


PI = compute_pi()
LN2 = Decimal(2).ln()


def sin_cos(x):
    getcontext().prec += 60  # |x| up to 2^128: keep 120 digits after the reduction
    r = x - (x / (2 * PI)).to_integral_value() * 2 * PI
    s, c, term, k = Decimal(0), Decimal(0), Decimal(1), 0
    while k < 400:
        if k % 2 == 0:
            c += term if k % 4 == 0 else -term
        else:
            s += term if k % 4 == 1 else -term
        k += 1
        term = term * r / k
        if abs(term) < EPS * EPS and k > 2:
            break
    getcontext().prec -= 60
    return +s, +c


def atan(x):
    if x < 0:
        return -atan(-x)
    if x > 1:
        return PI / 2 - atan(1 / x)
    halvings = 0
    while x > Decimal("0.1"):
        x = x / (1 + (1 + x * x).sqrt())
        halvings += 1
    x2, term, total, k = x * x, x, x, 1
    while abs(term) > EPS * EPS:
        term *= -x2
        total += term / (2 * k + 1)
        k += 1
    return total * (2 ** halvings)


def asin(x):
    if abs(x) == 1:
        return PI / 2 * x
    return atan(x / (1 - x * x).sqrt())


def atan2(y, x):
    if x > 0:
        return atan(y / x)
    if x < 0:
        return atan(y / x) + (PI if y >= 0 else -PI)
    return PI / 2 if y > 0 else -PI / 2


def exact_value(name, args):
    """Exact (or 120-digit) value as a Fraction or Decimal; args are Python floats (exact)."""
    d = [Decimal(a) for a in args]
    x = d[0]
    if name == "sin":
        return sin_cos(x)[0]
    if name == "cos":
        return sin_cos(x)[1]
    if name == "tan":
        s, c = sin_cos(x)
        return s / c
    if name == "atan":
        return atan(x)
    if name == "asin":
        return asin(x)
    if name == "acos":
        return PI / 2 - asin(x)
    if name == "atan2":
        return atan2(d[0], d[1])
    if name == "exp":
        return x.exp()
    if name == "exp2":
        if x == x.to_integral_value():
            return Fraction(2) ** int(x)
        return (x * LN2).exp()
    if name == "ln":
        return x.ln()
    if name == "log2":
        f = Fraction(x)
        if f.numerator == 1 or f.denominator == 1:
            n = f.numerator if f.denominator == 1 else f.denominator
            if n & (n - 1) == 0:
                k = n.bit_length() - 1
                return Fraction(k if f.denominator == 1 else -k)
        return x.ln() / LN2
    if name == "pow":
        xf, yf = Fraction(d[0]), Fraction(d[1])
        if yf.denominator == 1:
            return xf ** int(yf)  # exact
        return d[0] ** d[1]  # Decimal power: exact when representable, else correctly rounded
    if name == "hypot":
        return (d[0] * d[0] + d[1] * d[1]).sqrt()
    raise ValueError("unknown function " + name)


def round_f32(v):
    """Round a Fraction/Decimal to the nearest f32 (ties to even); returns bits."""
    f = Fraction(v)
    if f == 0:
        return 0
    sign = 0x80000000 if f < 0 else 0
    a = abs(f)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if Fraction(2) ** e > a:
        e -= 1
    e = max(e, -126)
    scale = Fraction(2) ** (e - 23)
    n = round(a / scale)  # Fraction.__round__: ties to even
    val = n * scale
    if val > Fraction(struct.unpack("<f", struct.pack("<I", 0x7F7FFFFF))[0]):
        return sign | 0x7F800000
    return sign | struct.unpack("<I", struct.pack("<f", float(val)))[0]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    print_only = "--print" in sys.argv
    lines = open(args[0]).read().split("\n") if args else sys.stdin.read().split("\n")
    bad = checked = 0
    for line in lines:
        parts = line.split()
        if not parts or parts[0].startswith("#"):
            continue
        name, nums = parts[0], [int(p, 16) for p in parts[1:]]
        n_args = 2 if name in ("atan2", "pow", "hypot") else 1
        inputs, got = nums[:n_args], (nums[n_args] if len(nums) > n_args else None)
        want = round_f32(exact_value(name, [f32(b) for b in inputs]))
        checked += 1
        if print_only or got is None:
            print(name, " ".join("%#010x" % b for b in inputs), "%#010x" % want)
        elif (got & 0x7FFFFFFF or want & 0x7FFFFFFF) and got != want:
            bad += 1
            print("WRONG %s(%s) = %#010x, correctly rounded %#010x"
                  % (name, ", ".join(repr(f32(b)) for b in inputs), got, want))
    if not print_only:
        print("%d checked, %d wrong" % (checked, bad))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
