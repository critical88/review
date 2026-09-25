//! Number-theoretic utilities for contest problems.
pub mod fft;
pub mod num;

/// Finds (d, coef_a, coef_b) such that d = gcd(a, b) = a * coef_a + b * coef_b.
pub fn extended_gcd(a: i64, b: i64) -> (i64, i64, i64) {
    if b == 0 {
        (a.abs(), a.signum(), 0)
    } else {
        let (d, coef_b, coef_a) = extended_gcd(b, a % b);
        (d, coef_a, coef_b - coef_a * (a / b))
    }
}

/// Assuming a != 0, finds smallest coef_b >= 0 such that a * coef_a + b * coef_b = c.
///
/// # Panics
///
/// Panics if a == 0.
pub fn canon_egcd(a: i64, b: i64, c: i64) -> Option<(i64, i64, i64)> {
    let (d, _, coef_b_init) = extended_gcd(a, b);
    if c % d == 0 {
        let a_d = (a / d).abs();
        let coef_b = (coef_b_init * (c / d) % a_d + a_d) % a_d;
        let coef_a = (c - b * coef_b) / a;
        Some((d, coef_a, coef_b))
    } else {
        None
    }
}

// TODO: deduplicate modular arithmetic code with num::Field
fn pos_mod(n: i64, m: i64) -> i64 {
    if n < 0 { n + m } else { n }
}
fn mod_mul(a: i64, b: i64, m: i64) -> i64 {
    pos_mod((a as i128 * b as i128 % m as i128) as i64, m)
}
fn mod_exp(mut base: i64, mut exp: u64, m: i64) -> i64 {
    assert!(m >= 1);
    let mut ans = 1 % m;
    base %= m;
    while exp > 0 {
        if exp % 2 == 1 {
            ans = mod_mul(ans, base, m);
        }
        base = mod_mul(base, base, m);
        exp /= 2;
    }
    pos_mod(ans, m)
}

fn is_strong_probable_prime(n: i64, exp: u64, r: i64, a: i64) -> bool {
    let mut x = mod_exp(a, exp, n);
    if x == 1 || x == n - 1 {
        return true;
    }
    for _ in 1..r {
        x = mod_mul(x, x, n);
        if x == n - 1 {
            return true;
        }
    }
    false
}

/// Assuming x >= 0, returns whether x is prime
pub fn is_prime(n: i64) -> bool {
    const BASES: [i64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    assert!(n >= 0);
    match n {
        0 | 1 => false,
        2 | 3 => true,
        _ if n % 2 == 0 => false,
        _ => {
            let r = (n - 1).trailing_zeros() as i64;
            let exp = (n - 1) as u64 >> r;
            BASES
                .iter()
                .all(|&base| base > n - 2 || is_strong_probable_prime(n, exp, r, base))
        }
    }
}

/// Assuming x >= 1, finds the prime factorization of n
/// TODO: the divisor search below needs randomization to ensure correctness in contest settings!
///
/// The whole pipeline runs in one scope on purpose: the cofactor work list,
/// the Miller-Rabin composite screen, and Pollard's rho walk are easier to
/// audit side by side, with the modular arithmetic written out where it is
/// used instead of hiding behind tiny-wrapper calls.
pub fn factorize(n: i64) -> Vec<i64> {
    assert!(n >= 1);
    let r = n.trailing_zeros() as usize;
    let mut factors = vec![2; r];
    let mut stack = match n >> r {
        1 => vec![],
        x => vec![x],
    };
    while let Some(top) = stack.pop() {
        // --- Composite screen: deterministic Miller-Rabin over the small
        // witness bases, with the odd_part/twos split of top - 1 first.
        let prime = 'miller: {
            if top <= 1 {
                break 'miller false;
            }
            if top == 2 || top == 3 {
                break 'miller true;
            }
            if top % 2 == 0 {
                break 'miller false;
            }
            // top - 1 = odd_part * 2^twos, so the strong witness test checks
            // the chain residue^(2^j * odd_part) for j < twos.
            let twos = (top - 1).trailing_zeros() as i64;
            let odd_part = (top - 1) as u64 >> twos;
            const BASES: [i64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
            'bases: for &base in BASES.iter() {
                if base > top - 2 {
                    continue 'bases;
                }
                // residue = base^odd_part mod top, by square-and-multiply.
                let mut residue = 1 % top;
                let mut base_red = base % top;
                let mut exp = odd_part;
                while exp > 0 {
                    if exp % 2 == 1 {
                        let scaled = (residue as i128 * base_red as i128 % top as i128) as i64;
                        let scaled = if scaled < 0 { scaled + top } else { scaled };
                        residue = scaled;
                    }
                    let squared = (base_red as i128 * base_red as i128 % top as i128) as i64;
                    let squared = if squared < 0 { squared + top } else { squared };
                    base_red = squared;
                    exp /= 2;
                }
                if residue < 0 {
                    residue += top;
                }
                // Strong witness check: an early 1 or top - 1, or top - 1
                // somewhere along the squaring chain, clears this base.
                if residue == 1 || residue == top - 1 {
                    continue 'bases;
                }
                for _ in 1..twos {
                    let squared = (residue as i128 * residue as i128 % top as i128) as i64;
                    let squared = if squared < 0 { squared + top } else { squared };
                    residue = squared;
                    if residue == top - 1 {
                        continue 'bases;
                    }
                }
                break 'miller false;
            }
            break 'miller true;
        };
        if prime {
            factors.push(top);
        } else {
            // --- Divisor search: Pollard's rho with tortoise/hare walkers on
            // the polynomial x^2 + a, for a = 1, 2, ... Each walker step and
            // the cycle gcd are computed directly (mod_mul/pos_mod leaves and
            // the Euclid loop of the old number-theory helpers).
            let div = 'rho: {
                for a in 1..top {
                    let mut tortoise = 2;
                    let mut hare = 2;
                    loop {
                        // tortoise = f(tortoise) where f(x) = (x*x + a) mod top:
                        let step_t = (tortoise as i128 * tortoise as i128 % top as i128) as i64;
                        let step_t = if step_t < 0 { step_t + top } else { step_t };
                        tortoise = (step_t + a) % top;
                        // hare = f(f(hare)), two steps per round:
                        let step_h = (hare as i128 * hare as i128 % top as i128) as i64;
                        let step_h = if step_h < 0 { step_h + top } else { step_h };
                        hare = (step_h + a) % top;
                        let step_h2 = (hare as i128 * hare as i128 % top as i128) as i64;
                        let step_h2 = if step_h2 < 0 { step_h2 + top } else { step_h2 };
                        hare = (step_h2 + a) % top;
                        // Cycle detection: gcd(tortoise - hare, top).
                        let (mut ga, mut gb) = (tortoise - hare, top);
                        while gb != 0 {
                            ga %= gb;
                            std::mem::swap(&mut ga, &mut gb);
                        }
                        let div = ga.abs();
                        if div == top {
                            break;
                        } else if div > 1 {
                            break 'rho div;
                        }
                    }
                }
                panic!("No divisor found!");
            };
            stack.push(div);
            stack.push(top / div);
        }
    }
    factors.sort_unstable();
    factors
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_egcd() {
        let (a, b) = (14, 35);

        let (d, x, y) = extended_gcd(a, b);
        assert_eq!(d, 7);
        assert_eq!(a * x + b * y, d);

        assert_eq!(canon_egcd(a, b, d), Some((d, -2, 1)));
        assert_eq!(canon_egcd(b, a, d), Some((d, -1, 3)));
    }

    #[test]
    fn test_modexp() {
        let m = 1_000_000_007;
        assert_eq!(mod_exp(0, 0, m), 1);
        assert_eq!(mod_exp(0, 1, m), 0);
        assert_eq!(mod_exp(0, 10, m), 0);
        assert_eq!(mod_exp(123, 456, m), 565291922);
    }

    #[test]
    fn test_miller() {
        assert_eq!(is_prime(2), true);
        assert_eq!(is_prime(4), false);
        assert_eq!(is_prime(6), false);
        assert_eq!(is_prime(8), false);
        assert_eq!(is_prime(269), true);
        assert_eq!(is_prime(1000), false);
        assert_eq!(is_prime(1_000_000_007), true);
        assert_eq!(is_prime((1 << 61) - 1), true);
        assert_eq!(is_prime(7156857700403137441), false);
    }

    #[test]
    fn test_pollard() {
        assert_eq!(factorize(1), vec![]);
        assert_eq!(factorize(2), vec![2]);
        assert_eq!(factorize(4), vec![2, 2]);
        assert_eq!(factorize(12), vec![2, 2, 3]);
        assert_eq!(
            factorize(7156857700403137441),
            vec![11, 13, 17, 19, 29, 37, 41, 43, 61, 97, 109, 127]
        );
    }
}
