"""
gspread.auth
~~~~~~~~~~~~

Simple authentication with OAuth.

The authentication entry points live on :class:`gspread.client.Client`.
This module keeps the historical ``gspread.auth`` API working by
forwarding every call to the client.

"""

import os
from pathlib import Path
from typing import Any, Dict, Iterable, Mapping, Optional, Tuple, Union

from google.auth.credentials import Credentials
from requests import Session

from .client import Client, FlowCallable
from .http_client import HTTPClient, HTTPClientType

# default scopes / file locations are provided by the client
DEFAULT_SCOPES = Client.DEFAULT_SCOPES
READONLY_SCOPES = Client.READONLY_SCOPES
DEFAULT_CONFIG_DIR: Path = Client.DEFAULT_CONFIG_DIR
DEFAULT_CREDENTIALS_FILENAME: Path = Client.DEFAULT_CREDENTIALS_FILENAME
DEFAULT_AUTHORIZED_USER_FILENAME: Path = Client.DEFAULT_AUTHORIZED_USER_FILENAME
DEFAULT_SERVICE_ACCOUNT_FILENAME: Path = Client.DEFAULT_SERVICE_ACCOUNT_FILENAME
GOOGLE_AUTH_API_KEY_AVAILABLE: bool = Client.GOOGLE_AUTH_API_KEY_AVAILABLE


def get_config_dir(
    config_dir_name: str = "gspread", os_is_windows: bool = os.name == "nt"
) -> Path:
    r"""Construct a config dir path.

    By default:
        * `%APPDATA%\gspread` on Windows
        * `~/.config/gspread` everywhere else

    """
    return Client.get_config_dir(
        config_dir_name=config_dir_name, os_is_windows=os_is_windows
    )


def authorize(
    credentials: Credentials,
    http_client: HTTPClientType = HTTPClient,
    session: Optional[Session] = None,
) -> Client:
    """Login to Google API using OAuth2 credentials.
    This is a shortcut/helper function which
    instantiates a client using `http_client`.
    By default :class:`gspread.HTTPClient` is used (but could also use
    :class:`gspread.BackOffHTTPClient` to avoid rate limiting).

    It can take an additional `requests.Session` object in order to provide
    you own session object.

    .. note::

       When providing your own `requests.Session` object,
       use the value `None` as `credentials`.

    :returns: An instance of the class produced by `http_client`.
    :rtype: :class:`gspread.client.Client`
    """

    return Client.authorize(
        credentials=credentials, http_client=http_client, session=session
    )


def local_server_flow(
    client_config: Mapping[str, Any], scopes: Iterable[str], port: int = 0
) -> Credentials:
    """Run an OAuth flow using a local server strategy.

    Creates an OAuth flow and runs `google_auth_oauthlib.flow.InstalledAppFlow.run_local_server <https://google-auth-oauthlib.readthedocs.io/en/latest/reference/google_auth_oauthlib.flow.html#google_auth_oauthlib.flow.InstalledAppFlow.run_local_server>`_.
    This will start a local web server and open the authorization URL in
    the user's browser.

    Pass this function to ``flow`` parameter of :meth:`~gspread.oauth` to run
    a local server flow.
    """
    return Client.local_server_flow(
        client_config=client_config, scopes=scopes, port=port
    )


def load_credentials(
    filename: Path = DEFAULT_AUTHORIZED_USER_FILENAME,
) -> Optional[Credentials]:
    return Client.load_credentials(filename=filename)


def store_credentials(
    creds: Credentials,
    filename: Path = DEFAULT_AUTHORIZED_USER_FILENAME,
    strip: str = "token",
) -> None:
    Client.store_credentials(creds=creds, filename=filename, strip=strip)


def oauth(
    scopes: Iterable[str] = DEFAULT_SCOPES,
    flow: FlowCallable = local_server_flow,
    credentials_filename: Union[str, Path] = DEFAULT_CREDENTIALS_FILENAME,
    authorized_user_filename: Union[str, Path] = DEFAULT_AUTHORIZED_USER_FILENAME,
    http_client: HTTPClientType = HTTPClient,
) -> Client:
    r"""Authenticate with OAuth Client ID.

    By default this function will use the local server strategy and open
    the authorization URL in the user's browser::

        gc = gspread.oauth()

    ``scopes`` parameter defaults to read/write scope available in
    ``gspread.auth.DEFAULT_SCOPES``. It's read/write for Sheets
    and Drive API::

        DEFAULT_SCOPES =[
            'https://www.googleapis.com/auth/spreadsheets',
            'https://www.googleapis.com/auth/drive'
        ]

    You can also use ``gspread.auth.READONLY_SCOPES`` for read only access.

    :rtype: :class:`gspread.client.Client`
    """
    return Client.oauth(
        scopes=scopes,
        flow=flow,
        credentials_filename=credentials_filename,
        authorized_user_filename=authorized_user_filename,
        http_client=http_client,
    )


def oauth_from_dict(
    credentials: Optional[Mapping[str, Any]] = None,
    authorized_user_info: Optional[Mapping[str, Any]] = None,
    scopes: Iterable[str] = DEFAULT_SCOPES,
    flow: FlowCallable = local_server_flow,
    http_client: HTTPClientType = HTTPClient,
) -> Tuple[Client, Dict[str, Any]]:
    r"""Authenticate with OAuth Client ID.

    This function requires you to pass the credentials directly as
    a python dict. After the first authentication the function returns
    the authenticated user info, this can be passed again to authenticate
    the user without the need to run the flow again.

    .. code-block:: python

        gc, authorized_user_info = gspread.oauth_from_dict(
            credentials=my_creds,
            authorized_user_info=my_auth_user
        )

    :rtype: (:class:`gspread.client.Client`, str)
    """
    return Client.oauth_from_dict(
        credentials=credentials,
        authorized_user_info=authorized_user_info,
        scopes=scopes,
        flow=flow,
        http_client=http_client,
    )


def service_account(
    filename: Union[Path, str] = DEFAULT_SERVICE_ACCOUNT_FILENAME,
    scopes: Iterable[str] = DEFAULT_SCOPES,
    http_client: HTTPClientType = HTTPClient,
) -> Client:
    """Authenticate using a service account.

    ``scopes`` parameter defaults to read/write scope available in
    ``gspread.auth.DEFAULT_SCOPES``. It's read/write for Sheets
    and Drive API::

        DEFAULT_SCOPES =[
            'https://www.googleapis.com/auth/spreadsheets',
            'https://www.googleapis.com/auth/drive'
        ]

    :rtype: :class:`gspread.client.Client`
    """
    return Client.service_account(
        filename=filename, scopes=scopes, http_client=http_client
    )


def service_account_from_dict(
    info: Mapping[str, Any],
    scopes: Iterable[str] = DEFAULT_SCOPES,
    http_client: HTTPClientType = HTTPClient,
) -> Client:
    """Authenticate using a service account (json).

    :rtype: :class:`gspread.client.Client`
    """
    return Client.service_account_from_dict(
        info=info, scopes=scopes, http_client=http_client
    )


def api_key(token: str, http_client: HTTPClientType = HTTPClient) -> Client:
    """Authenticate using an API key.

    Allows you to open public spreadsheet files.

    .. warning::

       This method only allows you to open public spreadsheet files.
       It does not work for private spreadsheet files.

    :rtype: :class:`gspread.client.Client`
    """
    return Client.api_key(token=token, http_client=http_client)
