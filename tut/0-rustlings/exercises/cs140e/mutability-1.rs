// FIXME: Make me compile! Diff budget: 1 line.

fn make_1(v: &mut u32) {
    *v = 1;
}

fn main() {
    let mut v = 5;
    make_1(&mut v);
}
