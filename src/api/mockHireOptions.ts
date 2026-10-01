import type { EffortOption, ToolOptions } from "../types";

// Demo catalogue for Phase 1, copied from the real sources on 2026-09-29 so the
// hire dialog shows what the tools actually accept. The Phase-2 backend reads
// these live instead:
//  - Claude Code: `claude --help` (--model aliases, --effort low|medium|high|xhigh|max)
//    and the defaults in ~/.claude/settings.json (model, modelSettings.*.effortLevel).
//  - Codex: ~/.codex/models_cache.json (models with visibility "list", each with its
//    own supported_reasoning_levels and default_reasoning_level) and model /
//    model_reasoning_effort in ~/.codex/config.toml.

const effort = (id: string, label: string, description: string): EffortOption => ({ id, label, description });

const CLAUDE_EFFORTS: readonly EffortOption[] = [
  effort("low", "Low", ""),
  effort("medium", "Medium", ""),
  effort("high", "High", ""),
  effort("xhigh", "Extra high", ""),
  effort("max", "Max", ""),
];

const CODEX_BASE: readonly EffortOption[] = [
  effort("low", "Low", "Fast responses with lighter reasoning"),
  effort("medium", "Medium", "Balances speed and reasoning depth for everyday tasks"),
  effort("high", "High", "Greater reasoning depth for complex problems"),
  effort("xhigh", "Extra high", "Extra high reasoning depth for complex problems"),
];
const CODEX_MAX: readonly EffortOption[] = [...CODEX_BASE, effort("max", "Max", "Maximum reasoning depth for the hardest problems")];
const CODEX_ULTRA: readonly EffortOption[] = [...CODEX_MAX, effort("ultra", "Ultra", "Maximum reasoning with automatic task delegation")];

export function makeHireOptions(): readonly ToolOptions[] {
  return [
    {
      tool: "claude-code",
      defaultModel: "claude-opus-5-5",
      models: [
        { id: "claude-opus-5-5", label: "Opus 5.5", efforts: CLAUDE_EFFORTS, defaultEffort: "high" },
        { id: "claude-fable-5-1", label: "Fable 5.1", efforts: CLAUDE_EFFORTS, defaultEffort: "medium" },
        { id: "claude-sonnet-5", label: "Sonnet 5", efforts: CLAUDE_EFFORTS, defaultEffort: "medium" },
        { id: "claude-haiku-4-5-20251001", label: "Haiku 4.5", efforts: CLAUDE_EFFORTS, defaultEffort: "medium" },
      ],
    },
    {
      tool: "codex",
      defaultModel: "gpt-6-astra",
      models: [
        { id: "gpt-6-astra", label: "GPT-6-Astra", efforts: CODEX_ULTRA, defaultEffort: "xhigh" },
        { id: "gpt-5.6-sol", label: "GPT-5.6-Sol", efforts: CODEX_ULTRA, defaultEffort: "low" },
        { id: "gpt-5.6-terra", label: "GPT-5.6-Terra", efforts: CODEX_ULTRA, defaultEffort: "medium" },
        { id: "gpt-5.6-luna", label: "GPT-5.6-Luna", efforts: CODEX_MAX, defaultEffort: "medium" },
        { id: "gpt-5.5", label: "GPT-5.5", efforts: CODEX_BASE, defaultEffort: "medium" },
      ],
    },
  ];
}
