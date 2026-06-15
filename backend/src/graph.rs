//! In-memory character/variant graph (DESIGN.md §7: "hold the character/variant graph in memory
//! as adjacency maps at startup").
//!
//! Identity-class edges (simplification / shinjitai / z-variant) are unioned into equivalence
//! classes via union-find. A word's **backbone key** is the sequence of its characters' class ids
//! - so 学校 (simp/ja) and 學校 (trad) share one key, and looking words up by key gives both the
//! cross-script match and the 同字 (orthographic) link in a single index hit.
//!
//! semantic-variant edges are deliberately excluded (suggestion-only; never expanded), which is
//! what stops the original's 夾→袷 over-fire.

use std::collections::HashMap;

use rusqlite::Connection;

pub struct VariantGraph {
    /// codepoint -> equivalence class id
    class_of: HashMap<u32, u32>,
    /// backbone key (dot-joined class/codepoint tokens) -> lexeme ids sharing that key
    form_index: HashMap<String, Vec<i64>>,
}

struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new() -> Self {
        Self { parent: Vec::new(), rank: Vec::new() }
    }
    fn make(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.rank.push(0);
        id
    }
    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize]; // path halving
            x = self.parent[x as usize];
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (ra, rb) = if self.rank[ra as usize] < self.rank[rb as usize] { (rb, ra) } else { (ra, rb) };
        self.parent[rb as usize] = ra;
        if self.rank[ra as usize] == self.rank[rb as usize] {
            self.rank[ra as usize] += 1;
        }
    }
}

impl VariantGraph {
    pub fn load(conn: &Connection) -> rusqlite::Result<Self> {
        // --- union-find over identity-class edges ---
        let mut uf = UnionFind::new();
        let mut idx: HashMap<u32, u32> = HashMap::new(); // codepoint -> uf index

        let intern = |uf: &mut UnionFind, idx: &mut HashMap<u32, u32>, cp: u32| -> u32 {
            *idx.entry(cp).or_insert_with(|| uf.make())
        };

        let mut stmt = conn.prepare(
            "SELECT child_cp, parent_cp FROM glyph_edge \
             WHERE type IN ('simplification','shinjitai','z-variant')",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32)))?;
        for row in rows {
            let (c, p) = row?;
            let ci = intern(&mut uf, &mut idx, c);
            let pi = intern(&mut uf, &mut idx, p);
            uf.union(ci, pi);
        }

        // codepoint -> stable class id (the uf root index)
        let mut class_of: HashMap<u32, u32> = HashMap::with_capacity(idx.len());
        for (&cp, &i) in &idx {
            class_of.insert(cp, uf.find(i));
        }

        let mut g = VariantGraph { class_of, form_index: HashMap::new() };

        // --- build the backbone-key index from every surface form ---
        let mut stmt = conn.prepare("SELECT form, lexeme_id FROM surface_form")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (form, lexeme_id) = row?;
            let key = g.key(&form);
            g.form_index.entry(key).or_default().push(lexeme_id);
        }
        for v in g.form_index.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        Ok(g)
    }

    /// Backbone key for a surface string: each char mapped to its class id (or its codepoint if
    /// it has no identity edges), so variant glyphs collapse to one key.
    pub fn key(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len() * 4);
        for ch in s.chars() {
            let cp = ch as u32;
            match self.class_of.get(&cp) {
                Some(cls) => {
                    out.push('c');
                    out.push_str(&cls.to_string());
                }
                None => {
                    out.push('u');
                    out.push_str(&cp.to_string());
                }
            }
            out.push('.');
        }
        out
    }

    /// Lexeme ids whose backbone key matches this string (cross-script + 同字).
    pub fn lexemes_by_key(&self, s: &str) -> &[i64] {
        self.form_index.get(&self.key(s)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn num_classes(&self) -> usize {
        self.form_index.len()
    }
}
