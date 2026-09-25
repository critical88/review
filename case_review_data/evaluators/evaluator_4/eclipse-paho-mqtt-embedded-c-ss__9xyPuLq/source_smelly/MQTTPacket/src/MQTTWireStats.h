/*******************************************************************************
 * Copyright (c) 2026 IBM Corp.
 *
 * All rights reserved. This program and the accompanying materials
 * are made available under the terms of the Eclipse Public License v2.0
 * and Eclipse Distribution License v1.0 which accompany this distribution.
 *
 * The Eclipse Public License is available at
 *    http://www.eclipse.org/legal/epl-v20.html
 * and the Eclipse Distribution License are available at
 *   http://www.eclipse.org/org/documents/edl-v10.php.
 *
 * Contributors:
 *    Sample Contributor - wire activity accounting for support dumps
 *******************************************************************************/

#ifndef MQTTWIRESTATS_H_
#define MQTTWIRESTATS_H_

#if defined(__cplusplus) /* If this is a C++ compiler, use C linkage */
extern "C" {
#endif

/* Number of MQTT packet types, including the reserved type 0. */
#define MQTTWIRESTATS_TYPE_COUNT 15

/**
 * Wire activity accounting record for the embedded client.  It is kept in
 * one place so that a support dump can report on the traffic handled by the
 * packet layer without needing a full logging implementation.
 */
typedef struct
{
	unsigned long sentCount[MQTTWIRESTATS_TYPE_COUNT]; /**< packets serialized out, by packet type */
	unsigned long receivedCount[MQTTWIRESTATS_TYPE_COUNT]; /**< packets decoded in, by packet type */
	unsigned long bytesOut; /**< wire bytes serialized for the current session */
	unsigned long bytesIn; /**< wire bytes decoded for the current session */
	unsigned long payloadBytesOut; /**< application payload bytes published */
	unsigned long payloadBytesIn; /**< application payload bytes delivered */
	unsigned long highWaterPayload; /**< largest single payload seen, either direction */
	unsigned long framesIn; /**< complete frames accepted from a transport */
	unsigned long rejects; /**< packets turned away before decoding completed */
	unsigned long sessionStarts; /**< connect packets serialized since boot */
	unsigned long decodedTopics; /**< topic filters and granted QoS slots decoded */
	unsigned char lastInspected; /**< packet type most recently rendered for an operator */
} MQTTWireStats;

/** The single, shared wire activity accounting record. */
extern MQTTWireStats wireStats;

void MQTTWireStats_note(int outbound, unsigned char packettype, int wirelen);

#if defined(__cplusplus) /* If this is a C++ compiler, use C linkage */
}
#endif

#endif /* MQTTWIRESTATS_H_ */
