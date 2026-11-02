use std::sync::Arc;

use serde::{Serialize, Serializer};

use crate::model::RandomSource;
use super::helpers::{Group, Tuple, shuffle};
use super::probe::{SearchTrace, TraceEvent, record};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor { pub combo_index: usize, pub odometer: Vec<usize>, pub entered: bool }

impl Serialize for Cursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.combo_index, &self.odometer, self.entered).serialize(serializer)
    }
}

pub struct Neighborhood {
    combos: Vec<Tuple>,
    pools: Arc<[Vec<usize>]>,
    orders: Vec<Option<Vec<Vec<usize>>>>,
    cursor: Cursor,
}

impl Neighborhood {
    pub fn new(combos: Vec<Tuple>, pools: impl Into<Arc<[Vec<usize>]>>, trace: &mut Option<SearchTrace>) -> Self {
        let pools = pools.into();
        record(trace, || TraceEvent::Neighborhood { combos: combos.clone(), pools: pools.to_vec() });
        let orders = vec![None; combos.len()];
        Self { combos, pools, orders, cursor: Cursor { combo_index: 0, odometer: Vec::new(), entered: false } }
    }
    pub fn position(&self) -> Cursor { self.cursor.clone() }
    pub fn exhausted(&self) -> bool { self.cursor.combo_index >= self.combos.len() }
    pub fn orders(&self) -> &[Option<Vec<Vec<usize>>>] { &self.orders }
    pub fn seek(&mut self, position: Cursor, trace: &mut Option<SearchTrace>) {
        record(trace, || TraceEvent::Seek { before: self.position(), after: position.clone() });
        self.cursor = position;
    }
    fn order<R: RandomSource>(&mut self, index: usize, random: &mut R, trace: &mut Option<SearchTrace>) {
        if self.orders[index].is_some() { return; }
        let combo = self.combos[index];
        let mut orders: Vec<Vec<usize>> = combo.as_slice().iter().map(|&slot| self.pools[slot].clone()).collect();
        for pool in &mut orders {
            let before = trace.as_ref().map(|_| pool.clone());
            shuffle(random, pool);
            if let Some(before) = before { record(trace, || TraceEvent::Shuffle { before, after: pool.clone() }); }
        }
        record(trace, || TraceEvent::PoolOrders { index, combo, orders: orders.clone() });
        self.orders[index] = Some(orders);
    }

    pub fn take<R: RandomSource>(&mut self, base: &[usize], limit: usize, random: &mut R,
        trace: &mut Option<SearchTrace>) -> (Vec<Group>, usize) {
        let before = trace.as_ref().map(|_| self.position());
        let mut groups = Vec::new();
        let mut taken = 0;
        while taken < limit && !self.exhausted() {
            let index = self.cursor.combo_index;
            let combo = self.combos[index];
            self.order(index, random, trace);
            let pools = self.orders[index].as_ref().expect("entered combination order");
            if pools.iter().any(Vec::is_empty) {
                self.cursor.combo_index += 1;
                self.cursor.entered = false;
                continue;
            }
            if !self.cursor.entered {
                self.cursor.odometer.clear();
                self.cursor.odometer.resize(combo.len(), 0);
                self.cursor.entered = true;
            }
            let inner = combo.len() - 1;
            let inner_pool = &pools[inner];
            let inner_equipped = base[combo.as_slice()[inner]];
            let mut candidates = Vec::new();
            while self.cursor.entered && taken < limit {
                let mut items = [0; 4];
                let mut keep_row = false;
                for axis in 0..inner {
                    let id = pools[axis][self.cursor.odometer[axis]];
                    if id == base[combo.as_slice()[axis]] { keep_row = true; break; }
                    items[axis] = id;
                }
                if !keep_row {
                    let mut item_index = self.cursor.odometer[inner];
                    while item_index < inner_pool.len() && taken < limit {
                        let id = inner_pool[item_index];
                        item_index += 1;
                        if id == inner_equipped { continue; }
                        items[inner] = id;
                        candidates.push(Tuple::new(&items[..combo.len()]));
                        taken += 1;
                    }
                    self.cursor.odometer[inner] = item_index;
                    if item_index < inner_pool.len() { break; }
                }
                self.cursor.odometer[inner] = 0;
                let mut carry = inner;
                while carry > 0 {
                    carry -= 1;
                    self.cursor.odometer[carry] += 1;
                    if self.cursor.odometer[carry] < pools[carry].len() { break; }
                    self.cursor.odometer[carry] = 0;
                }
                if inner == 0 || (carry == 0 && self.cursor.odometer[0] == 0) {
                    self.cursor.combo_index += 1;
                    self.cursor.entered = false;
                }
            }
            if !candidates.is_empty() { groups.push(Group { combo, candidates }); }
        }
        if let Some(before) = before {
            record(trace, || TraceEvent::Take { base: base.to_vec(), limit, before, after: self.position(), groups: groups.clone(), total: taken });
        }
        (groups, taken)
    }
}
