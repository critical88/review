# Maintenance request

I maintain the embedded MQTT packet layer in `MQTTPacket/src` — the code that turns MQTT packets into wire bytes and back. It bit us last round. When we decided rejected packets should be counted separately instead of as received, the change reached over more of the packet sources than I want to admit, and the non-blocking read path was missed — a customer's dump showed frames nobody had counted. The next change on the list is to stop counting keep-alive traffic in the byte totals, and I am not looking forward to the same survey of every function that pokes the record.

I first ran into this while working around `MQTTSerialize_connect` in `MQTTPacket/src/MQTTConnectClient.c`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
