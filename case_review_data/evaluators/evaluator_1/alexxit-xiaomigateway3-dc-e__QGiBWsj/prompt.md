# Maintenance request

I maintain this integration and just finished onboarding another gateway generation for the third time, and the same chore bit every time: the handful of fields that make up a child device's identity — model, did, type, plus the assorted hardware addresses and firmware versions — get unpacked, renamed and re-passed as parallel arguments and bare tuples everywhere in the adapter layer. This smells like a missing type to me.

I first ran into this while working around `XGateway` in `custom_components/xiaomi_gateway3/core/gate/base.py`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
