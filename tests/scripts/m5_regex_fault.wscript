// An invalid regex pattern is a trappable fault (programming error), not
// a script Result.
// error: invalid regex pattern

use regex

fn main() {
    println(regex::is_match("(unclosed", "x"))
}
