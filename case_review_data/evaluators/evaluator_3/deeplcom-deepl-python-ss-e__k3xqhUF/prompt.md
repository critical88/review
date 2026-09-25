# Maintenance request

We maintain the Python client library for the DeepL API. Every endpoint method in both client generations — text translation, the document workflow, the rephrase and correct write-backs, usage and language queries, the two generations of glossary management, style rules, translation memories, and the pre-signed file transfers used for translation-memory import/export — runs its HTTP call through a common request mechanism and gets back a status code, the raw content, and the parsed response JSON.

I first ran into this while working around `DeepLClient` in `deepl/deepl_client.py`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
