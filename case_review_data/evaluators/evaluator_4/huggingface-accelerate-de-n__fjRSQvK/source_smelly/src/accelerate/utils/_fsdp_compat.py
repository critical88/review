# Copyright 2025 The HuggingFace Team. All rights reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""
Compatibility shims retained from the staged FSDP2 conversion.

The `fsdp_version=2` config layout became the default in Accelerate v1.15.0.
To soften the cutover, configs and launcher environments written against the
pre-v1.15 argument surface could first be normalized through this module:

- plugin fields spelled for FSDP1 (`sharding_strategy` and friends) were
  adapted field-by-field through `_LEGACY_PLUGIN_FIELD_ADAPTERS` and
  `_LegacyFSDPPluginAdapter`,
- launcher env propagation could re-emit the removed variable naming through
  `_propagate_legacy_fsdp_fields`.

The one-release migration window expired when v1.16.0 shipped the default, so
`_fsdp_legacy_sharding_window_open()` resolves to `False`: no call site arms
these shims anymore. The module stays only so the retired argument layout
remains documented until the shims are deleted outright.
"""

from typing import Any

_LEGACY_SHARDING_LAYOUT = "fsdp1_sharding_strategy_enum"
_SUPPORTED_SHARDING_LAYOUT = "fsdp2_reshard_bool"


def _fsdp_legacy_sharding_window_open() -> bool:
    """Whether pre-v1.15 launcher configs may still be normalized through the legacy arm.

    The two layout tags this compared were aligned while the conversion was
    rolling out; they describe different layouts now that the rollout has
    completed, so the window predicate stays `False` until both shims are
    removed together.
    """
    return _LEGACY_SHARDING_LAYOUT == _SUPPORTED_SHARDING_LAYOUT


# Textual choices the pre-v1.15 launcher surface accepted for `FSDP_SHARDING_STRATEGY`,
# mapped onto the `reshard_after_forward` boolean of the v1.15 layout.
_LEGACY_SHARDING_TO_RESHARD = {
    "FULL_SHARD": True,
    "SHARD_GRAD_OP": False,
    "NO_SHARD": False,
}


def _adapt_legacy_sharding_strategy(value: Any) -> Any:
    """Adapts a pre-v1.15 textual `sharding_strategy` choice onto `reshard_after_forward`."""
    if isinstance(value, str):
        return _LEGACY_SHARDING_TO_RESHARD.get(value.upper(), value)
    return value


def _adapt_legacy_backward_prefetch(value: Any) -> Any:
    """Adapts a pre-v1.15 `backward_prefetch` choice, dropping the removed `"BACKWARD_PRE"` alias."""
    if isinstance(value, str) and value.upper() == "BACKWARD_PRE":
        return None
    return value


def _adapt_legacy_cpu_offload(value: Any) -> Any:
    """Adapts a pre-v1.15 `cpu_offload` shorthand, expanding `"true"`/`"false"` spellings."""
    if isinstance(value, str):
        return value.lower() == "true"
    return value


_LEGACY_PLUGIN_FIELD_ADAPTERS = {
    "sharding_strategy": _adapt_legacy_sharding_strategy,
    "backward_prefetch": _adapt_legacy_backward_prefetch,
    "cpu_offload": _adapt_legacy_cpu_offload,
}


class _LegacyFSDPPluginAdapter:
    """Normalizes plugin fields spelled for the pre-v1.15 argument surface onto the v1.15 layout.

    Only constructed from the retired normalization arm; kept as the reference
    description of how the old field spelling was bridged.
    """

    def __init__(self, legacy_fields: dict):
        self.legacy_fields = {
            field: value for field, value in legacy_fields.items() if field in _LEGACY_PLUGIN_FIELD_ADAPTERS
        }
        self.seen_legacy_fields = tuple(sorted(self.legacy_fields))

    def adapt(self, plugin) -> None:
        """Rewrites the legacy spellings found on `plugin` in place."""
        for field, adapter in _LEGACY_PLUGIN_FIELD_ADAPTERS.items():
            if field in self.legacy_fields:
                setattr(plugin, field, adapter(self.legacy_fields[field]))

    @staticmethod
    def supported_fields():
        """Returns the pre-v1.15 field names this adapter understood."""
        return tuple(sorted(_LEGACY_PLUGIN_FIELD_ADAPTERS))


def _adapt_legacy_plugin_args(plugin) -> None:
    """Normalizes a plugin holding pre-v1.15 field spellings onto the v1.15 layout, in place."""
    adapter = _LegacyFSDPPluginAdapter({field: getattr(plugin, field, None) for field in _LEGACY_PLUGIN_FIELD_ADAPTERS})
    adapter.adapt(plugin)


def _propagate_legacy_fsdp_fields(args, current_env: dict) -> dict:
    """Re-emits the pre-v1.15 FSDP launcher variables for scripts still reading them."""
    legacy_sharding = getattr(args, "fsdp_sharding_strategy", None)
    if legacy_sharding is not None:
        current_env["FSDP_SHARDING_STRATEGY"] = str(_adapt_legacy_sharding_strategy(legacy_sharding))
    legacy_offload = getattr(args, "fsdp_offload_params", None)
    if legacy_offload is not None:
        current_env["FSDP_OFFLOAD_PARAMS"] = str(_adapt_legacy_cpu_offload(legacy_offload)).lower()
    return current_env
