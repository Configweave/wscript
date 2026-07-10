// fmt format specs: width/fill/align, zero-pad, precision, int bases.
// expect: [      42]
// expect: [42      ]
// expect: [  mid   ]
// expect: [***ab***]
// expect: [00000042]
// expect: [-0000042]
// expect: 3.14
// expect: 0.50
// expect: trunc
// expect: ff FF 101010 52
// expect: [    2a]

fn main() -> int {
    println(fmt("[{:>8}]", 42))
    println(fmt("[{:<8}]", 42))
    println(fmt("[{:^8}]", "mid"))
    println(fmt("[{:*^8}]", "ab"))
    println(fmt("[{:08}]", 42))
    println(fmt("[{:08}]", -42))
    println(fmt("{:.2}", 3.14159))
    println(fmt("{:.2}", 0.5))
    println(fmt("{:.5}", "truncated"))
    println(fmt("{:x} {:X} {:b} {:o}", 255, 255, 42, 42))
    println(fmt("[{:>6x}]", 42))
    0
}
