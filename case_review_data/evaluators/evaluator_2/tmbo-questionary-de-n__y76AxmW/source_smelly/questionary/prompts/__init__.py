from questionary.prompts import autocomplete
from questionary.prompts import checkbox
from questionary.prompts import confirm
from questionary.prompts import password
from questionary.prompts import path
from questionary.prompts import press_any_key_to_continue
from questionary.prompts import rawselect
from questionary.prompts import select
from questionary.prompts import text

# This flag routed the deprecated 1.x question names onto their pre-2.0
# factories while installations were still moving from prompt_toolkit 2 to
# prompt_toolkit 3. The migration is finished, the flag is never enabled
# anymore and only documents where the legacy dispatch belongs.
USE_LEGACY_PROMPTS = False

# Deprecated 1.x question names and the factories that recreate their
# pre-2.0 behaviour (see prompt_by_name below).
LEGACY_PROMPTS = {
    "list": select.legacy_list_prompt,
    "rawlist": rawselect.legacy_rawlist_prompt,
    "input": text.legacy_input_prompt,
}

AVAILABLE_PROMPTS = {
    "autocomplete": autocomplete.autocomplete,
    "confirm": confirm.confirm,
    "text": text.text,
    "select": select.select,
    "rawselect": rawselect.rawselect,
    "password": password.password,
    "checkbox": checkbox.checkbox,
    "path": path.path,
    "press_any_key_to_continue": press_any_key_to_continue.press_any_key_to_continue,
    # backwards compatible names
    "list": select.select,
    "rawlist": rawselect.rawselect,
    "input": text.text,
}


def prompt_by_name(name):
    if USE_LEGACY_PROMPTS:
        legacy = LEGACY_PROMPTS.get(name)
        if legacy is not None:
            return legacy
    return AVAILABLE_PROMPTS.get(name)
