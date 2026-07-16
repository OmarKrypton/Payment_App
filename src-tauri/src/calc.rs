#![allow(non_snake_case)]
use crate::models::{CalcResult, FormData, RateRow};

fn parse_amt(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

fn parse_rate(s: &str) -> f64 {
    s.trim_end_matches('%').parse::<f64>().unwrap_or(0.0)
}

fn compute_rate_total(rows: &[RateRow]) -> f64 {
    let total: f64 = rows
        .iter()
        .map(|r| parse_amt(&r.amount) * parse_rate(&r.rate) / 100.0)
        .sum();
    (total * 100.0).round() / 100.0
}

pub fn recalculate(data: &FormData) -> CalcResult {
    let gv = |k: &str| -> f64 {
        match k {
            "val_1A" => parse_amt(&data.val_1A),
            "val_1B" => parse_amt(&data.val_1B),
            "val_1C" => parse_amt(&data.val_1C),
            "val_1D" => parse_amt(&data.val_1D),
            "vat_rate" => parse_rate(&data.vat_rate),
            "val_2A" => parse_amt(&data.val_2A),
            "val_2B" => parse_amt(&data.val_2B),
            "val_2C" => parse_amt(&data.val_2C),
            "ret_rate" => parse_rate(&data.ret_rate),
            "val_4A" => parse_amt(&data.val_4A),
            "val_4B" => parse_amt(&data.val_4B),
            "val_4C" => parse_amt(&data.val_4C),
            "temp_rate" => parse_rate(&data.temp_rate),
            "val_5A" => parse_amt(&data.val_5A),
            "val_5C" => parse_amt(&data.val_5C),
            "wht_rate" => parse_rate(&data.wht_rate),
            "val_6A" => parse_amt(&data.val_6A),
            "oth_rate" => parse_rate(&data.oth_rate),
            "val_8A" => parse_amt(&data.val_8A),
            "soc_rate" => parse_rate(&data.soc_rate),
            "val_12A" => parse_amt(&data.val_12A),
            "val_7A" => parse_amt(&data.val_7A),
            "val_10A" => parse_amt(&data.val_10A),
            _ => 0.0,
        }
    };

    let c_1A = gv("val_1A");
    let c_1B = gv("val_1B");
    let c_1C = gv("val_1C");
    let c_1D = gv("val_1D");

    let c_1E = if data.vat_manual {
        compute_rate_total(&data.vat_rows)
    } else {
        (c_1B * gv("vat_rate") / 100.0 * 100.0).round() / 100.0
    };
    let c_1G = c_1B + c_1E;
    let c_1F = c_1A + c_1B + c_1C - c_1D + c_1E;

    let c_2A = gv("val_2A");
    let c_2B = gv("val_2B");
    let c_2C = gv("val_2C");
    let c_2D = c_2B + c_2C;
    let c_2E = c_2A - c_2D;

    let c_3A = c_1F - c_2D;

    let c_4A = gv("val_4A");
    let c_4B = (c_1B * gv("ret_rate") / 100.0 * 100.0).round() / 100.0;
    let c_4C = gv("val_4C");
    let c_4D = c_4A + c_4B - c_4C;

    let temp_rate = gv("temp_rate");
    let c_5A = gv("val_5A");
    let c_5B = (c_1B * temp_rate / 100.0 * 100.0).round() / 100.0;
    let c_5C = gv("val_5C");
    let c_5D = c_5A + c_5B - c_5C;

    let c_6A = gv("val_6A");
    let c_6B = if data.wht_manual {
        compute_rate_total(&data.wht_rows)
    } else {
        (c_1B * gv("wht_rate") / 100.0 * 100.0).round() / 100.0
    };
    let c_6C = c_6A + c_6B;
    let c_6D = c_1G - c_6B;

    let c_8A = gv("val_8A");
    let c_8B = if data.oth_manual {
        compute_rate_total(&data.oth_rows)
    } else {
        (c_1B * gv("oth_rate") / 100.0 * 100.0).round() / 100.0
    };
    let c_8C = c_8A + c_8B;

    let c_12A = gv("val_12A");
    let c_12B = if data.soc_manual {
        compute_rate_total(&data.soc_rows)
    } else {
        (c_1B * gv("soc_rate") / 100.0 * 100.0).round() / 100.0
    };
    let c_12C = c_12A + c_12B;

    let total_deductions = c_4B + c_5B + c_6B + c_8B + c_12B;

    let c_7A = gv("val_7A");
    let c_7B = c_7A + c_6A;
    let c_9A = c_3A - c_4D - c_5D - c_6C - c_7A - c_8C - c_12C;
    let c_10A = gv("val_10A");
    let c_11A = c_7A + c_10A + c_2A;
    let c_11B = c_11A + c_6C;

    // ── Import section ──
    let import_vat_rate = parse_rate(&data.import_vat_rate);
    let import_commercial = parse_amt(&data.import_commercial_amount);
    let import_c1 = parse_amt(&data.import_cost_1);
    let import_c2 = parse_amt(&data.import_cost_2);
    let import_c3 = parse_amt(&data.import_cost_3);
    let import_total_costs = (import_c1 + import_c2 + import_c3) * 100.0 / 100.0;

    let mut import_entry_sum = 0.0;
    let mut import_total_vat = 0.0;
    let mut import_total_wht = 0.0;
    for entry in &data.import_entries {
        let amt = parse_amt(&entry.amount);
        let vat = (amt * import_vat_rate / 100.0 * 100.0).round() / 100.0;
        let wht = if entry.free_wht {
            0.0
        } else {
            (amt * parse_rate(&entry.wht_rate) / 100.0 * 100.0).round() / 100.0
        };
        import_entry_sum += amt;
        import_total_vat += vat;
        import_total_wht += wht;
    }
    let import_gross_amount = ((import_commercial + import_total_costs + import_entry_sum) * 100.0).round() / 100.0;
    import_total_vat = (import_total_vat * 100.0).round() / 100.0;
    import_total_wht = (import_total_wht * 100.0).round() / 100.0;
    let import_grand_total = ((import_gross_amount + import_total_vat) * 100.0).round() / 100.0;
    let import_grand_net = ((import_gross_amount + import_total_vat - import_total_wht) * 100.0).round() / 100.0;

    CalcResult {
        c_1A,
        c_1B,
        c_1C,
        c_1D,
        c_1E,
        c_1F,
        c_1G,
        c_2A,
        c_2B,
        c_2C,
        c_2D,
        c_2E,
        c_3A,
        c_4A,
        c_4B,
        c_4C,
        c_4D,
        c_5A,
        c_5B,
        c_5C,
        c_5D,
        c_6A,
        c_6B,
        c_6C,
        c_6D,
        c_8A,
        c_8B,
        c_8C,
        c_12A,
        c_12B,
        c_12C,
        total_deductions,
        c_7A,
        c_7B,
        c_9A,
        c_10A,
        c_11A,
        c_11B,
        import_total_costs,
        import_gross_amount,
        import_total_vat,
        import_total_wht,
        import_grand_total,
        import_grand_net,
    }
}
