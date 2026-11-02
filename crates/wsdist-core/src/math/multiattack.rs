use crate::model::Job;

use super::{python_max, python_min};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExpectedHits {
    pub main: f64,
    pub sub: f64,
    pub daken: f64,
    pub kick: f64,
    pub zanshin: f64,
}

/// get_ma_rate.py:183–284. This is the legacy eight-*expected-landed-hit*
/// approximation, not an eight-attempt proc enumeration. The first main-hand
/// proc's max placement intentionally differs from all later procs.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn get_ma_rate3(main_job: Job, nhits: f64, qa: f64, ta: f64, da: f64, oa_list: &[f64; 9], _fua_list: &[f64; 9], dual_wield: bool, hitrate_matrix: &[[f64; 2]; 2], ranged_hitrate2: f64, daken: f64, kickattacks: f64, zanshin: f64, zanhasso: f64, zanshin_hitrate: f64, zanshin_oa2: f64, striking_flourish: bool, ternary_flourish: bool, tp_round: bool) -> ExpectedHits {
    let mut main_hits = 0.0;
    let mut sub_hits = 0.0;
    let mut daken_hits = 0.0;
    let mut kickattack_hits = 0.0;
    let hitrate11 = hitrate_matrix[0][0];
    let hitrate21 = hitrate_matrix[0][1];
    let hitrate12 = hitrate_matrix[1][0];
    let hitrate22 = hitrate_matrix[1][1];
    main_hits += 1.0 * hitrate11;
    main_hits += (nhits - 1.0) * hitrate12;
    if dual_wield && main_hits + sub_hits < 8.0 {
        sub_hits += 1.0 * hitrate21;
    }
    let (qa_main, ta_main, da_main) = if striking_flourish { (0.0, 0.0, 1.0) }
        else if ternary_flourish { (0.0, 1.0, 0.0) }
        else { (qa, ta, da) };
    let oa3_main = oa_list[0];
    let oa2_main = oa_list[1];
    let oa8_sub = oa_list[2];
    let oa7_sub = oa_list[3];
    let oa6_sub = oa_list[4];
    let oa5_sub = oa_list[5];
    let oa4_sub = oa_list[6];
    let oa3_sub = oa_list[7];
    let oa2_sub = oa_list[8];

    main_hits += (python_max(0.0, python_min(3.0, 8.0 - (main_hits + sub_hits)) * qa_main)
        + python_max(0.0, python_min(2.0, 8.0 - (main_hits + sub_hits)) * (1.0 - qa_main) * ta_main)
        + python_max(0.0, python_min(1.0, 8.0 - (main_hits + sub_hits)) * (1.0 - qa_main) * (1.0 - ta_main) * da_main)
        + python_max(0.0, python_min(2.0, 8.0 - (main_hits + sub_hits)) * (1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * oa3_main)
        + python_max(0.0, python_min(1.0, 8.0 - (main_hits + sub_hits)) * (1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * (1.0 - oa3_main) * oa2_main)) * hitrate12;

    let zanshin_hitrate = if tp_round { zanshin_hitrate } else { 0.0 };
    let zanshin_bonus = ((1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * (1.0 - oa3_main) * (1.0 - oa2_main) * (1.0 - hitrate12) * zanshin * zanshin_oa2 * 2.0
        + (1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * (1.0 - oa3_main) * (1.0 - oa2_main) * (1.0 - hitrate12) * zanshin * (1.0 - zanshin_oa2)) * zanshin_hitrate;
    let zanshin_sam_bonus = ((1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * (1.0 - oa3_main) * (1.0 - oa2_main) * hitrate12 * zanhasso * zanshin_oa2 * 2.0
        + (1.0 - qa_main) * (1.0 - ta_main) * (1.0 - da_main) * (1.0 - oa3_main) * (1.0 - oa2_main) * hitrate12 * zanhasso * (1.0 - zanshin_oa2)) * zanshin_hitrate * if main_job == Job::Sam { 1.0 } else { 0.0 };
    let zanshin_hits = zanshin_bonus + zanshin_sam_bonus;

    if dual_wield {
        sub_hits += (python_max(0.0, python_min(3.0, 8.0 - (sub_hits + main_hits))) * qa
            + python_max(0.0, python_min(2.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * ta
            + python_max(0.0, python_min(1.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * da
            + python_max(0.0, python_min(7.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * oa8_sub
            + python_max(0.0, python_min(6.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * oa7_sub
            + python_max(0.0, python_min(5.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * (1.0 - oa7_sub) * oa6_sub
            + python_max(0.0, python_min(4.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * (1.0 - oa7_sub) * (1.0 - oa6_sub) * oa5_sub
            + python_max(0.0, python_min(3.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * (1.0 - oa7_sub) * (1.0 - oa6_sub) * (1.0 - oa5_sub) * oa4_sub
            + python_max(0.0, python_min(2.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * (1.0 - oa7_sub) * (1.0 - oa6_sub) * (1.0 - oa5_sub) * (1.0 - oa4_sub) * oa3_sub
            + python_max(0.0, python_min(1.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa8_sub) * (1.0 - oa7_sub) * (1.0 - oa6_sub) * (1.0 - oa5_sub) * (1.0 - oa4_sub) * (1.0 - oa3_sub) * oa2_sub) * hitrate22;
    } else if nhits > 1.0 {
        main_hits += (python_max(0.0, python_min(3.0, 8.0 - (sub_hits + main_hits))) * qa
            + python_max(0.0, python_min(2.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * ta
            + python_max(0.0, python_min(1.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * da
            + python_max(0.0, python_min(2.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * oa3_main
            + python_max(0.0, python_min(1.0, 8.0 - (sub_hits + main_hits))) * (1.0 - qa) * (1.0 - ta) * (1.0 - da) * (1.0 - oa3_main) * oa2_main) * hitrate12;
    }
    if tp_round {
        let remaining_hits_available = 8.0 - (main_hits + sub_hits);
        kickattack_hits += python_min(python_max(0.0, remaining_hits_available), 1.0) * kickattacks * hitrate12;
        // Source subtracts the proc rate, not expected landed kick hits.
        daken_hits += python_min(python_max(0.0, remaining_hits_available - kickattacks), 1.0) * daken * ranged_hitrate2;
    }
    ExpectedHits { main: main_hits, sub: sub_hits, daken: daken_hits, kick: kickattack_hits, zanshin: zanshin_hits }
}
