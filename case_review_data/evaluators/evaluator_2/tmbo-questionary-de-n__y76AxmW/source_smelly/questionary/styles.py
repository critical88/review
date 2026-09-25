from typing import List
from typing import Optional

import prompt_toolkit.styles

from questionary.constants import DEFAULT_STYLE

# Compatibility switch of the retired style pipeline. Every 1.x question
# merged the user style straight into the default style; this flag kept that
# path around for installations which had not moved to the shared merge
# helper yet. It is pinned off and nothing inside questionary reads or
# writes it besides the checkbox question.
LEGACY_STYLE_COMPAT = False


def merge_styles_default(styles: List[Optional[prompt_toolkit.styles.Style]]):
    """Merge a list of styles with the Questionary default style."""
    filtered_styles: list[prompt_toolkit.styles.BaseStyle] = [DEFAULT_STYLE]
    # prompt_toolkit's merge_styles works with ``None`` elements, but it's
    # type-hints says it doesn't.
    filtered_styles.extend([s for s in styles if s is not None])
    return prompt_toolkit.styles.merge_styles(filtered_styles)


def merge_styles_1_x(style: Optional[prompt_toolkit.styles.Style]):
    """Merge a user style into the Questionary default style the 1.x way.

    Every 1.x question handed both styles straight to prompt_toolkit, which
    breaks for ``None`` styles on the current prompt_toolkit. This is kept
    for the legacy checkbox look, which did not disable the inverted bottom
    toolbar yet.
    """
    return prompt_toolkit.styles.merge_styles([DEFAULT_STYLE, style])
