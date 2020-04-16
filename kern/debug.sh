#!/usr/bin/env bash
foo=$(mktemp)

cat > "$foo" <<EOF
target remote :1234
add-symbol-file build/kernel.elf 0x80000

define fn
si
x/20i \$pc - 12
end

EOF

exec rust-gdb target/aarch64-unknown-none/release/kernel -x "$foo"
