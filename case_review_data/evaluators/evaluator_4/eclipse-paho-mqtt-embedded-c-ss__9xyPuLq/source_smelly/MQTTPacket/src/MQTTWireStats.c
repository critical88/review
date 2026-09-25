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

#include "MQTTWireStats.h"

/* The single, shared wire activity accounting record. */
MQTTWireStats wireStats = {0};

/**
 * Notes a packet which has been serialized, or decoded, as part of its normal
 * path through the packet layer.
 * @param outbound 1 if the packet was serialized for sending, 0 if it was decoded
 * @param packettype the MQTT packet type
 * @param wirelen the total number of wire bytes in the packet
 */
void MQTTWireStats_note(int outbound, unsigned char packettype, int wirelen)
{
	if (outbound)
	{
		wireStats.sentCount[packettype]++;
		wireStats.bytesOut += wirelen;
	}
	else
	{
		wireStats.receivedCount[packettype]++;
		wireStats.bytesIn += wirelen;
	}
}
