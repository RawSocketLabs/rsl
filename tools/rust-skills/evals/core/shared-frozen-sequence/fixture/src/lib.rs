#[derive(Clone)]
pub struct FilterPlan {
    coefficients: Vec<f32>,
}

impl FilterPlan {
    pub fn new(coefficients: Vec<f32>) -> Self {
        Self { coefficients }
    }

    pub fn coefficients(&self) -> &[f32] {
        &self.coefficients
    }
}

#[cfg(test)]
mod tests {
    use super::FilterPlan;

    #[test]
    fn cloned_plans_preserve_coefficients() {
        let plan = FilterPlan::new(vec![0.25, 0.5, 0.25]);
        let worker_plan = plan.clone();

        assert_eq!(worker_plan.coefficients(), plan.coefficients());
    }
}
