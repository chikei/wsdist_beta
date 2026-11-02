use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Serialize, Serializer};

use crate::catalog::Catalog;
use crate::model::{Abilities, CalcError, GearId, ModelState, OptimizeOptions, Player, RandomSource};
use crate::stats::{Stat, Stats};

pub const MAX_SWAP_SLOTS: usize = 4;
pub const PARALLEL_CANDIDATE_THRESHOLD: usize = 20_000;
pub type DtVector = [f64; 6];
pub type InternedSet = [usize; 16];

/// Small, inline slot/item tuples. Candidate enumeration never allocates a tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tuple {
    values: [usize; MAX_SWAP_SLOTS],
    len: u8,
}

impl Tuple {
    pub fn new(values: &[usize]) -> Self {
        assert!(values.len() <= MAX_SWAP_SLOTS);
        let mut result = Self { values: [0; MAX_SWAP_SLOTS], len: values.len() as u8 };
        result.values[..values.len()].copy_from_slice(values);
        result
    }
    pub fn as_slice(&self) -> &[usize] { &self.values[..usize::from(self.len)] }
    pub fn len(&self) -> usize { usize::from(self.len) }
    pub fn is_empty(&self) -> bool { self.len == 0 }
}

impl Serialize for Tuple {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group { pub combo: Tuple, pub candidates: Vec<Tuple> }

impl Serialize for Group {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.combo, &self.candidates).serialize(serializer)
    }
}

pub fn normalize_options(options: Option<&OptimizeOptions>) -> OptimizeOptions {
    let defaults = OptimizeOptions::default();
    let options = options.unwrap_or(&defaults);
    OptimizeOptions {
        iterations: options.iterations.max(1), max_swap_slots: options.max_swap_slots.clamp(1, 4),
        restart_count: options.restart_count.max(1), seed: options.seed,
        dt_step: if options.dt_step > 1.0 { options.dt_step } else { 1.0 }, workers: options.workers.max(0),
    }
}

pub fn restart_options(options: &OptimizeOptions, index: usize) -> Result<OptimizeOptions, CalcError> {
    let mut single = options.clone();
    single.restart_count = 1;
    single.seed = options.seed.map(|seed| {
        u32::try_from(u64::from(seed) + index as u64).map_err(|_| CalcError::legacy(
            "optimize", "seed", "ValueError", "Seed must be between 0 and 2**32 - 1",
        ))
    }).transpose()?;
    Ok(single)
}

pub fn dt_vector(stats: &Stats) -> DtVector {
    [Stat::Pdt, Stat::Mdt, Stat::Dt, Stat::Pdt2, Stat::Mdt2, Stat::Dt2].map(|stat| stats.get(stat))
}

pub fn piece_dt(catalog: &Catalog, gear: GearId) -> DtVector {
    let piece = catalog.piece(gear);
    [Stat::Pdt, Stat::Mdt, Stat::Dt, Stat::Pdt2, Stat::Mdt2, Stat::Dt2].map(|stat| piece.stat(stat))
}

pub fn dt_sum(vectors: impl IntoIterator<Item = DtVector>) -> DtVector {
    let mut total = [0.0; 6];
    for vector in vectors { for index in 0..6 { total[index] += vector[index]; } }
    total
}

pub fn dt_totals(vector: DtVector) -> [f64; 2] {
    let pdt = vector[0] + vector[2];
    let mdt = vector[1] + vector[2];
    // Python max(-50, NaN) retains its first operand.
    [if pdt > -50.0 { pdt } else { -50.0 } + vector[3] + vector[5],
     if mdt > -50.0 { mdt } else { -50.0 } + vector[4] + vector[5]]
}

pub fn dt_for_player(player: &Player) -> [f64; 2] {
    let stats = &player.stats;
    let pdt = stats.get(Stat::Pdt) + stats.get(Stat::Dt);
    let mdt = stats.get(Stat::Mdt) + stats.get(Stat::Dt);
    [if pdt < -50.0 { -50.0 } else { pdt } + (stats.get(Stat::Pdt2) + stats.get(Stat::Dt2)),
     if mdt < -50.0 { -50.0 } else { mdt } + (stats.get(Stat::Mdt2) + stats.get(Stat::Dt2))]
}

pub fn can_use_fast_dt_gate(abilities: &Abilities) -> Result<bool, CalcError> {
    Ok(crate::math::python_int(abilities.aftermath(), "optimize", "Aftermath")? == 0.0)
}

/// The deduplicated combinations-with-replacement walk, not size-first subsets.
pub fn slot_combos(slot_count: usize, max_swap_slots: usize) -> Vec<Tuple> {
    if max_swap_slots == 0 { return vec![Tuple::new(&[])]; }
    assert!(max_swap_slots <= MAX_SWAP_SLOTS);
    if slot_count == 0 { return Vec::new(); }
    let mut padded = [0; MAX_SWAP_SLOTS];
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let mut unique = [0; MAX_SWAP_SLOTS];
        let mut len = 0;
        for &axis in &padded[..max_swap_slots] {
            if len == 0 || unique[len - 1] != axis { unique[len] = axis; len += 1; }
        }
        let combo = Tuple::new(&unique[..len]);
        if seen.insert(combo) { result.push(combo); }
        let Some(axis) = (0..max_swap_slots).rev().find(|&axis| padded[axis] + 1 < slot_count) else { break; };
        let next = padded[axis] + 1;
        padded[axis..max_swap_slots].fill(next);
    }
    result
}

#[derive(Clone, Debug)]
pub struct InternedPieces {
    pub pieces: Vec<GearId>,
    pub by_identity: Vec<Option<usize>>,
}

impl InternedPieces {
    pub fn id(&self, gear: GearId) -> usize { self.by_identity[gear.index()].expect("reachable gear") }
}

/// Full record equality includes ordered jobs and session-local skill overrides.
fn piece_hash(piece: &crate::model::GearPiece, state: &ModelState) -> u64 {
    let bits = |value: f64| if value == 0.0 { 0 } else { value.to_bits() };
    let mut hash = DefaultHasher::new();
    piece.name.hash(&mut hash); piece.name2.hash(&mut hash); piece.jobs.hash(&mut hash);
    piece.kind.hash(&mut hash); state.skill(piece).hash(&mut hash);
    bits(piece.dmg).hash(&mut hash); bits(piece.delay).hash(&mut hash); piece.rank.hash(&mut hash);
    piece.wsc.is_some().hash(&mut hash);
    if let Some(wsc) = piece.wsc { wsc.stat.hash(&mut hash); bits(wsc.percent).hash(&mut hash); }
    let mut stat_hash = 0_u64;
    for stat in &piece.stats {
        let mut entry = DefaultHasher::new();
        stat.stat.hash(&mut entry); bits(stat.value).hash(&mut entry);
        stat_hash = stat_hash.wrapping_add(entry.finish());
    }
    piece.stats.len().hash(&mut hash); stat_hash.hash(&mut hash);
    hash.finish()
}

pub fn intern_pieces<'a>(catalog: &Catalog, state: &ModelState, groups: impl IntoIterator<Item = &'a [GearId]>) -> InternedPieces {
    let mut result = InternedPieces { pieces: Vec::new(), by_identity: vec![None; catalog.pieces.len()] };
    let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();
    for group in groups {
        for &gear in group {
            if result.by_identity[gear.index()].is_some() { continue; }
            let piece = catalog.piece(gear);
            let bucket = buckets.entry(piece_hash(piece, state)).or_default();
            let id = bucket.iter().copied().find(|&id| piece.value_equal(catalog.piece(result.pieces[id]), state))
                .unwrap_or_else(|| {
                    let id = result.pieces.len();
                    result.pieces.push(gear);
                    bucket.push(id);
                    id
                });
            result.by_identity[gear.index()] = Some(id);
        }
    }
    result
}

pub fn shuffle<R: RandomSource, T>(random: &mut R, values: &mut [T]) {
    for index in (1..values.len()).rev() { let selected = random.bounded(index + 1); values.swap(index, selected); }
}

/// Borrowed, ordered shares: splitting a group does not copy its candidates.
#[derive(Clone, Copy, Debug)]
pub struct GroupSlice<'a> { pub combo: Tuple, pub candidates: &'a [Tuple] }

pub fn group_slices(groups: &[Group]) -> Vec<GroupSlice<'_>> {
    groups.iter().map(|group| GroupSlice { combo: group.combo, candidates: &group.candidates }).collect()
}

pub fn split_groups(groups: &[Group], total: usize, parts: usize) -> Vec<Vec<GroupSlice<'_>>> {
    assert!(parts > 0);
    let target = total.div_ceil(parts).max(1);
    let mut shares = Vec::new();
    let mut share = Vec::new();
    let mut filled = 0;
    for group in groups {
        let mut start = 0;
        while start < group.candidates.len() {
            let take = (target - filled).min(group.candidates.len() - start);
            share.push(GroupSlice { combo: group.combo, candidates: &group.candidates[start..start + take] });
            filled += take;
            start += take;
            if filled >= target { shares.push(share); share = Vec::new(); filled = 0; }
        }
    }
    if !share.is_empty() { shares.push(share); }
    shares
}
