/*
 * This file is part of SmartProxy <https://github.com/salarcode/SmartProxy>,
 * Copyright (C) 2024 Salar Khalilzadeh <salar2k@gmail.com>
 *
 * SmartProxy is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License version 3 as
 * published by the Free Software Foundation.
 *
 * SmartProxy is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with SmartProxy.  If not, see <http://www.gnu.org/licenses/>.
 */
import { SpecialRequestApplyProxyMode } from '../core/definitions';
import { ProxyEngineSpecialRequests } from '../core/ProxyEngineSpecialRequests';

export class SubscriptionFetch {

	/**
	 * Reads the remote text of a subscription source. This is the transport
	 * shared by both the proxy-server subscriptions and the proxy-rule
	 * subscriptions, so all connection fields arrive as individual parameters:
	 * `obfuscation` and `format` are carried along for the parse step.
	 */
	public static fetchSubscriptionText(
		url: string,
		username: string,
		password: string,
		obfuscation: string,
		format: number,
		applyProxy: SpecialRequestApplyProxyMode,
		success: Function,
		fail?: Function
	) {
		if (applyProxy !== null)
			// mark this request as special
			ProxyEngineSpecialRequests.setSpecialUrl(url, applyProxy);

		let fetchRequest: RequestInit = {
			method: 'GET',
			cache: 'no-store'
		};
		if (username) {
			let pass = atob(password);
			fetchRequest.headers =
			{
				'Authorization': 'Basic ' + btoa(username + ':' + pass)
			};
		}
		fetch(url, fetchRequest)
			.then(async (response) => {
				let responseText = await response.text();
				if (success)
					success(responseText, response.status, response.statusText);
			})
			.catch((error) => {
				if (fail)
					fail(error);
			});
	}
}
