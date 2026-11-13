//! Replay a PTY log into a terminal and print each cell's background on a few rows.
use kiln_term::{GridSize, Terminal};
use std::sync::Arc;
fn main() {
    let path = std::env::args().nth(1).expect("log path");
    let bytes = std::fs::read(path).unwrap();
    let t = Terminal::new(GridSize { cols: 165, rows: 50 }, Arc::new(|_: &[u8]| {}));
    t.feed(&bytes);
    let g = t.lock();
    let sel = g.selection.is_some();
    for line in 0..12 {
        let row = &g.grid()[alacritty_terminal::index::Line(line)];
        let mut bgs = std::collections::BTreeMap::new();
        for c in 0..165 { *bgs.entry(format!("{:?}", row[alacritty_terminal::index::Column(c)].bg)).or_insert(0) += 1; }
        println!("{line}: {bgs:?}");
    }
    println!("selection: {sel}");
}
