use super::*;

pub(super) struct AssassinStrategy;

impl GrandClassStrategy for AssassinStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Assassin
    }

    fn definition(&self) -> GrandClassDefinition {
        standard_definition(
            GrandClass::Assassin,
            "杀阶冠位",
            "Assassin",
            false,
            "杀阶戴冠战需要选择主冠位，副冠位可选",
        )
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        extra::default_built_in_rules()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        Some(RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick)
    }
}
