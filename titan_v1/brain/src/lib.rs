use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ProcessResult {
    pub output: String,
    pub risk_score: f64,
    pub requires_approval: bool,
}

#[wasm_bindgen]
impl ProcessResult {
    #[wasm_bindgen(constructor)]
    pub fn new(output: String, risk_score: f64, requires_approval: bool) -> ProcessResult {
        ProcessResult {
            output,
            risk_score,
            requires_approval,
        }
    }
}

#[wasm_bindgen]
pub fn analyze_instruction(input: &str) -> ProcessResult {
    let lower = input.to_lowercase();
    
    // Calculate risk based on keywords
    let mut risk = 0.5; // Base risk
    
    if lower.contains("delete") || lower.contains("remove") {
        risk += 0.3;
    }
    if lower.contains("sudo") || lower.contains("admin") {
        risk += 0.2;
    }
    if lower.contains("auto") || lower.contains("schedule") {
        risk -= 0.2;
    }
    
    risk = risk.clamp(0.0, 1.0);
    
    let requires_approval = risk > 0.3;
    
    ProcessResult::new(
        format!("TITAN_LOGIC: Processed '{}' | Risk: {:.2}", input, risk),
        risk,
        requires_approval,
    )
}

#[wasm_bindgen]
pub fn process_state(input: &str) -> String {
    format!("TITAN_LOGIC: Processed instruction '{}'. Ready for execution.", input)
}
