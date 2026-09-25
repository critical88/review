"""
gspread.client
~~~~~~~~~~~~~~

This module contains Client class responsible for managing spreadsheet files

"""

import json
import os
from datetime import datetime
from http import HTTPStatus
from pathlib import Path
from typing import (
    Any,
    Dict,
    Iterable,
    List,
    Mapping,
    Optional,
    Protocol,
    Tuple,
    Union,
)

from google.auth.credentials import Credentials
from google.oauth2.credentials import Credentials as OAuthCredentials
from google.oauth2.service_account import Credentials as SACredentials
from google_auth_oauthlib.flow import InstalledAppFlow
from requests import Response, Session

try:
    from google.auth.api_key import Credentials as APIKeyCredentials

    GOOGLE_AUTH_API_KEY_AVAILABLE = True
except ImportError:
    GOOGLE_AUTH_API_KEY_AVAILABLE = False

from .exceptions import APIError, SpreadsheetNotFound
from .http_client import HTTPClient, HTTPClientType, ParamsType
from .spreadsheet import Spreadsheet
from .urls import (
    DRIVE_FILES_API_V3_COMMENTS_URL,
    DRIVE_FILES_API_V3_URL,
    SPREADSHEET_VALUES_URL,
)
from .utils import (
    ExportFormat,
    MimeType,
    ValueInputOption,
    extract_id_from_url,
    finditem,
    quote,
)


class FlowCallable(Protocol):
    """Protocol for OAuth flow callables."""

    def __call__(
        self, client_config: Mapping[str, Any], scopes: Iterable[str], port: int = 0
    ) -> OAuthCredentials: ...


class Client(HTTPClient):
    """An instance of this class Manages Spreadsheet files

    It is used to:
        - open/create/list/delete spreadsheets
        - authenticate with OAuth2, service accounts or API keys
        - store, load and convert credentials
        - send the HTTP requests to the Google API
        - create/delete/list spreadsheet permission
        - etc

    It is the gspread entry point.
    It will handle creating necessary :class:`~gspread.models.Spreadsheet` instances.

    Because it is the single object every user holds, it carries the whole
    session: the credentials, the HTTP transport and the spreadsheet
    registry are all part of the same class.
    """

    DEFAULT_SCOPES = [
        "https://www.googleapis.com/auth/spreadsheets",
        "https://www.googleapis.com/auth/drive",
    ]

    READONLY_SCOPES = [
        "https://www.googleapis.com/auth/spreadsheets.readonly",
        "https://www.googleapis.com/auth/drive.readonly",
    ]

    GOOGLE_AUTH_API_KEY_AVAILABLE = GOOGLE_AUTH_API_KEY_AVAILABLE

    @staticmethod
    def get_config_dir(
        config_dir_name: str = "gspread", os_is_windows: bool = os.name == "nt"
    ) -> Path:
        r"""Construct a config dir path.

        By default:
            * `%APPDATA%\gspread` on Windows
            * `~/.config/gspread` everywhere else

        """
        if os_is_windows:
            return Path(os.environ["APPDATA"], config_dir_name)
        else:
            return Path(Path.home(), ".config", config_dir_name)

    DEFAULT_CONFIG_DIR = get_config_dir()

    DEFAULT_CREDENTIALS_FILENAME = DEFAULT_CONFIG_DIR / "credentials.json"
    DEFAULT_AUTHORIZED_USER_FILENAME = DEFAULT_CONFIG_DIR / "authorized_user.json"
    DEFAULT_SERVICE_ACCOUNT_FILENAME = DEFAULT_CONFIG_DIR / "service_account.json"

    def __init__(
        self,
        auth: Credentials,
        session: Optional[Session] = None,
        http_client: HTTPClientType = HTTPClient,
    ) -> None:
        # The client holds the transport state itself (session, auth,
        # timeout) and, for backward compatibility with callers passing
        # their own transport, a fully functional transport instance.
        super().__init__(auth=auth, session=session)
        self.http_client = http_client(auth, session)

    @staticmethod
    def local_server_flow(
        client_config: Mapping[str, Any],
        scopes: Iterable[str],
        port: int = 0,
    ) -> OAuthCredentials:
        """Run an OAuth flow using a local server strategy.

        Creates an OAuth flow and runs `google_auth_oauthlib.flow.InstalledAppFlow.run_local_server <https://google-auth-oauthlib.readthedocs.io/en/latest/reference/google_auth_oauthlib.flow.html#google_auth_oauthlib.flow.InstalledAppFlow.run_local_server>`_.
        This will start a local web server and open the authorization URL in
        the user's browser.
        """
        flow = InstalledAppFlow.from_client_config(client_config, scopes)
        return flow.run_local_server(port=port)

    @staticmethod
    def load_credentials(
        filename: Path = DEFAULT_AUTHORIZED_USER_FILENAME,
    ) -> Optional[Credentials]:
        if filename.exists():
            return OAuthCredentials.from_authorized_user_file(filename)

        return None

    @staticmethod
    def store_credentials(
        creds: OAuthCredentials,
        filename: Path = DEFAULT_AUTHORIZED_USER_FILENAME,
        strip: str = "token",
    ) -> None:
        filename.parent.mkdir(parents=True, exist_ok=True)
        with filename.open("w") as f:
            f.write(creds.to_json(strip))

    @classmethod
    def convert_credentials(cls, credentials: Credentials) -> Credentials:
        module = credentials.__module__
        cls_name = credentials.__class__.__name__
        if "oauth2client" in module and cls_name == "ServiceAccountCredentials":
            return cls._convert_service_account(credentials)
        elif "oauth2client" in module and cls_name in (
            "OAuth2Credentials",
            "AccessTokenCredentials",
            "GoogleCredentials",
        ):
            return cls._convert_oauth(credentials)
        elif isinstance(credentials, Credentials):
            return credentials

        raise TypeError(
            "Credentials need to be from either oauth2client or from google-auth."
        )

    @staticmethod
    def _convert_oauth(credentials: Any) -> Credentials:
        return OAuthCredentials(
            credentials.access_token,
            credentials.refresh_token,
            credentials.id_token,
            credentials.token_uri,
            credentials.client_id,
            credentials.client_secret,
            credentials.scopes,
        )

    @staticmethod
    def _convert_service_account(credentials: Any) -> Credentials:
        data = credentials.serialization_data
        data["token_uri"] = credentials.token_uri
        scopes = credentials._scopes.split() or [
            "https://www.googleapis.com/auth/drive",
            "https://spreadsheets.google.com/feeds",
        ]

        return SACredentials.from_service_account_info(data, scopes=scopes)

    @classmethod
    def authorize(
        cls,
        credentials: Credentials,
        http_client: HTTPClientType = HTTPClient,
        session: Optional[Session] = None,
    ) -> "Client":
        """Login to Google API using OAuth2 credentials.
        This is a shortcut/helper factory which instantiates a client
        using `http_client`.
        """
        return cls(auth=credentials, http_client=http_client, session=session)

    @classmethod
    def oauth(
        cls,
        scopes: Iterable[str] = DEFAULT_SCOPES,
        flow: FlowCallable = local_server_flow,
        credentials_filename: Union[str, Path] = DEFAULT_CREDENTIALS_FILENAME,
        authorized_user_filename: Union[str, Path] = DEFAULT_AUTHORIZED_USER_FILENAME,
        http_client: HTTPClientType = HTTPClient,
    ) -> "Client":
        r"""Authenticate with OAuth Client ID.

        By default this method will use the local server strategy and open
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
        Obviously any method of ``gspread`` that updates a spreadsheet
        **will not work** in this case::

            gc = gspread.oauth(scopes=gspread.auth.READONLY_SCOPES)

            sh = gc.open("A spreadsheet")
            sh.sheet1.update_acell('A1', '42')   # <-- this will not work

        If you're storing your user credentials in a place other than the
        default, you may provide a path to that file like so::

            gc = gspread.oauth(
                credentials_filename='/alternative/path/credentials.json',
                authorized_user_filename='/alternative/path/authorized_user.json',
            )

        :rtype: :class:`gspread.client.Client`
        """
        creds = cls.load_credentials(filename=Path(authorized_user_filename))

        if not isinstance(creds, Credentials):
            with open(credentials_filename) as json_file:
                client_config = json.load(json_file)
            creds = flow(client_config=client_config, scopes=scopes)
            cls.store_credentials(
                creds, filename=Path(authorized_user_filename)
            )

        return cls(auth=creds, http_client=http_client)

    @classmethod
    def oauth_from_dict(
        cls,
        credentials: Optional[Mapping[str, Any]] = None,
        authorized_user_info: Optional[Mapping[str, Any]] = None,
        scopes: Iterable[str] = DEFAULT_SCOPES,
        flow: FlowCallable = local_server_flow,
        http_client: HTTPClientType = HTTPClient,
    ) -> Tuple["Client", Dict[str, Any]]:
        r"""Authenticate with OAuth Client ID.

        This method requires you to pass the credentials directly as
        a python dict. After the first authentication the method returns
        the authenticated user info, this can be passed again to authenticate
        the user without the need to run the flow again.

        .. code-block:: python

            gc, authorized_user_info = Client.oauth_from_dict(
                credentials=my_creds,
                authorized_user_info=my_auth_user
            )

        :rtype: (:class:`gspread.client.Client`, str)
        """
        if authorized_user_info is not None:
            creds = OAuthCredentials.from_authorized_user_info(
                authorized_user_info, scopes
            )
        elif credentials is not None:
            creds = flow(client_config=credentials, scopes=scopes)
        else:
            raise ValueError("no credentials object supplied")

        client = cls(auth=creds, http_client=http_client)

        # must return the creds to the user
        # must strip the token an use the dedicated method from Credentials
        # to return a dict "safe to store".
        return (client, creds.to_json("token"))

    @classmethod
    def service_account(
        cls,
        filename: Union[Path, str] = DEFAULT_SERVICE_ACCOUNT_FILENAME,
        scopes: Iterable[str] = DEFAULT_SCOPES,
        http_client: HTTPClientType = HTTPClient,
    ) -> "Client":
        """Authenticate using a service account.

        ``scopes`` parameter defaults to read/write scope available in
        ``gspread.auth.DEFAULT_SCOPES``. It's read/write for Sheets
        and Drive API::

            DEFAULT_SCOPES =[
                'https://www.googleapis.com/auth/spreadsheets',
                'https://www.googleapis.com/auth/drive'
            ]

        :param str filename: The path to the service account json file.
        :param list scopes:
        :type http_client: :class:`gspread.http_client.HTTPClient`
        :param http_client: A factory function that returns a client class.
            Defaults to :class:`gspread.http_client.HTTPClient` (but could also use
            :class:`gspread.http_client.BackOffHTTPClient` to avoid rate limiting)

        :rtype: :class:`gspread.client.Client`
        """
        creds = SACredentials.from_service_account_file(filename, scopes=scopes)
        return cls(auth=creds, http_client=http_client)

    @classmethod
    def service_account_from_dict(
        cls,
        info: Mapping[str, Any],
        scopes: Iterable[str] = DEFAULT_SCOPES,
        http_client: HTTPClientType = HTTPClient,
    ) -> "Client":
        """Authenticate using a service account (json).

        :param info (Mapping[str, str]): The service account info in Google format
        :param list scopes:
        :type http_client: :class:`gspread.http_client.HTTPClient`
        :param http_client: A factory function that returns a client class.
            Defaults to :class:`gspread.http_client.HTTPClient` (but could also use
            :class:`gspread.http_client.BackOffHTTPClient` to avoid rate limiting)

        :rtype: :class:`gspread.client.Client`
        """
        creds = SACredentials.from_service_account_info(
            info=info,
            scopes=scopes,
        )
        return cls(auth=creds, http_client=http_client)

    @classmethod
    def api_key(cls, token: str, http_client: HTTPClientType = HTTPClient) -> "Client":
        """Authenticate using an API key.

        Allows you to open public spreadsheet files.

        .. warning::

           This method only allows you to open public spreadsheet files.
           It does not work for private spreadsheet files.

        :param token str: The actual API key to use
        :type http_client: :class:`gspread.http_client.HTTPClient`
        :param http_client: A factory function that returns a client class.
            Defaults to :class:`gspread.http_client.HTTPClient` (but could also use
            :class:`gspread.http_client.BackOffHTTPClient` to avoid rate limiting)

        """
        if cls.GOOGLE_AUTH_API_KEY_AVAILABLE is False:
            raise NotImplementedError(
                "api_key is only available with package google.auth>=2.15.0. "
                'Install it with "pip install google-auth>=2.15.0".'
            )
        creds = APIKeyCredentials(token)
        return cls(auth=creds, http_client=http_client)

    @property
    def expiry(self) -> Optional[datetime]:
        """Returns the expiry date of the curenlty loaded credentials

        :returns: (optional) datetime the expiry date time object.

        .. note::

           It only applies to gspread client created using oauth
        """
        return self.http_client.auth.expiry

    def set_timeout(
        self, timeout: Optional[Union[float, Tuple[float, float]]] = None
    ) -> None:
        """How long to wait for the server to send
        data before giving up, as a float, or a ``(connect timeout,
        read timeout)`` tuple.

        Use value ``None`` to restore default timeout

        Value for ``timeout`` is in seconds (s).
        """
        self.http_client.set_timeout(timeout)

    def get_file_drive_metadata(self, id: str) -> Any:
        """Get the metadata from the Drive API for a specific file
        This method is mainly here to retrieve the create/update time
        of a file (these metadata are only accessible from the Drive API).
        """
        return self.http_client.get_file_drive_metadata(id)

    def get_spreadsheet_values(
        self,
        spreadsheet_id: str,
        range_name: str,
        params: Optional[ParamsType] = None,
    ) -> List[List[str]]:
        """Get the values of a range of cells from the given spreadsheet.

        Reads the values from the given spreadsheet without opening the
        spreadsheet nor one of its worksheets first: only the spreadsheet
        ID, as found in the URL, is needed.

        :param str spreadsheet_id: The spreadsheet ID.
        :param str range_name: The range to read, in A1 notation (the sheet
            name may be provided using the absolute A1 notation).
        :param dict params: (optional) The HTTP params for the GET request.
        :returns: The matrix of values, as a list of rows.
        """
        url = SPREADSHEET_VALUES_URL % (spreadsheet_id, quote(range_name))
        response = self.http_client.request(
            "get", url, params=params or {"majorDimension": "ROWS"}
        )
        return response.json().get("values", [])

    def update_spreadsheet_values(
        self,
        spreadsheet_id: str,
        range_name: str,
        values: List[List[Any]],
        params: Optional[ParamsType] = None,
    ) -> Any:
        """Write the given values to a range of cells of the given spreadsheet.

        Writes the values to the given spreadsheet without opening the
        spreadsheet nor one of its worksheets first: only the spreadsheet
        ID, as found in the URL, is needed.

        :param str spreadsheet_id: The spreadsheet ID.
        :param str range_name: The range to update, in A1 notation (the sheet
            name may be provided using the absolute A1 notation).
        :param list values: The matrix of values to write, as a list of rows.
        :param dict params: (optional) The HTTP params for the PUT request.
            Defaults to the ``RAW`` value input option.
        :returns: The response body of the update.
        """
        url = SPREADSHEET_VALUES_URL % (spreadsheet_id, quote(range_name))
        body: Dict[str, Any] = {"values": values}
        response = self.http_client.request(
            "put",
            url,
            params=params or {"valueInputOption": ValueInputOption.raw},
            json=body,
        )
        return response.json()

    def list_spreadsheet_files(
        self, title: Optional[str] = None, folder_id: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """List all the spreadsheet files

        Will list all spreadsheet files owned by/shared with this user account.

        :param str title: Filter only spreadsheet files with this title
        :param str folder_id: Only look for spreadsheet files in this folder
            The parameter ``folder_id`` can be obtained from the URL when looking at
            a folder in a web browser as follow:
            ``https://drive.google.com/drive/u/0/folders/<folder_id>``

        :returns: a list of dicts containing the keys id, name, createdTime and modifiedTime.
        """
        files, _ = self._list_spreadsheet_files(title=title, folder_id=folder_id)
        return files

    def _list_spreadsheet_files(
        self, title: Optional[str] = None, folder_id: Optional[str] = None
    ) -> Tuple[List[Dict[str, Any]], Response]:
        files = []
        page_token = ""
        url = DRIVE_FILES_API_V3_URL

        query = f'mimeType="{MimeType.google_sheets}"'
        if title:
            query += f' and name = "{title}"'
        if folder_id:
            query += f' and parents in "{folder_id}"'

        params: ParamsType = {
            "q": query,
            "pageSize": 1000,
            "supportsAllDrives": True,
            "includeItemsFromAllDrives": True,
            "fields": "kind,nextPageToken,files(id,name,createdTime,modifiedTime)",
        }

        while True:
            if page_token:
                params["pageToken"] = page_token

            response = self.http_client.request("get", url, params=params)
            response_json = response.json()
            files.extend(response_json["files"])

            page_token = response_json.get("nextPageToken", None)

            if page_token is None:
                break

        return files, response

    def open(self, title: str, folder_id: Optional[str] = None) -> Spreadsheet:
        """Opens a spreadsheet.

        :param str title: A title of a spreadsheet.
        :param str folder_id: (optional) If specified can be used to filter
            spreadsheets by parent folder ID.
        :returns: a :class:`~gspread.models.Spreadsheet` instance.

        If there's more than one spreadsheet with same title the first one
        will be opened.

        :raises gspread.SpreadsheetNotFound: if no spreadsheet with
                                             specified `title` is found.

        >>> gc.open('My fancy spreadsheet')
        """
        spreadsheet_files, response = self._list_spreadsheet_files(title, folder_id)
        try:
            properties = finditem(
                lambda x: x["name"] == title,
                spreadsheet_files,
            )
        except StopIteration as ex:
            raise SpreadsheetNotFound(response) from ex

        # Drive uses different terminology
        properties["title"] = properties["name"]

        return Spreadsheet(self.http_client, properties)

    def open_by_key(self, key: str) -> Spreadsheet:
        """Opens a spreadsheet specified by `key` (a.k.a Spreadsheet ID).

        :param str key: A key of a spreadsheet as it appears in a URL in a browser.
        :returns: a :class:`~gspread.models.Spreadsheet` instance.

        >>> gc.open_by_key('0BmgG6nO_6dprdS1MN3d3MkdPa142WFRrdnRRUWl1UFE')
        """
        try:
            spreadsheet = Spreadsheet(self.http_client, {"id": key})
        except APIError as ex:
            if ex.response.status_code == HTTPStatus.NOT_FOUND:
                raise SpreadsheetNotFound(ex.response) from ex
            if ex.response.status_code == HTTPStatus.FORBIDDEN:
                raise PermissionError from ex
            raise ex
        return spreadsheet

    def open_by_url(self, url: str) -> Spreadsheet:
        """Opens a spreadsheet specified by `url`.

        :param str url: URL of a spreadsheet as it appears in a browser.

        :returns: a :class:`~gspread.models.Spreadsheet` instance.

        :raises gspread.SpreadsheetNotFound: if no spreadsheet with
                                             specified `url` is found.

        >>> gc.open_by_url('https://docs.google.com/spreadsheet/ccc?key=0Bm...FE&hl')
        """
        return self.open_by_key(extract_id_from_url(url))

    def openall(self, title: Optional[str] = None) -> List[Spreadsheet]:
        """Opens all available spreadsheets.

        :param str title: (optional) If specified can be used to filter
            spreadsheets by title.

        :returns: a list of :class:`~gspread.models.Spreadsheet` instances.
        """
        spreadsheet_files = self.list_spreadsheet_files(title)

        if title:
            spreadsheet_files = [
                spread for spread in spreadsheet_files if title == spread["name"]
            ]

        return [
            Spreadsheet(self.http_client, dict(title=x["name"], **x))
            for x in spreadsheet_files
        ]

    def create(self, title: str, folder_id: Optional[str] = None) -> Spreadsheet:
        """Creates a new spreadsheet.

        :param str title: A title of a new spreadsheet.
        :param str folder_id: Id of the folder where we want to save
            the spreadsheet.

        :returns: a :class:`~gspread.models.Spreadsheet` instance.

        """
        payload: Dict[str, Any] = {
            "name": title,
            "mimeType": MimeType.google_sheets,
        }

        params: ParamsType = {
            "supportsAllDrives": True,
        }

        if folder_id is not None:
            payload["parents"] = [folder_id]

        r = self.http_client.request(
            "post", DRIVE_FILES_API_V3_URL, json=payload, params=params
        )
        spreadsheet_id = r.json()["id"]
        return self.open_by_key(spreadsheet_id)

    def export(self, file_id: str, format: str = ExportFormat.PDF) -> bytes:
        """Export the spreadsheet in the given format.

        :param str file_id: The key of the spreadsheet to export

        :param str format: The format of the resulting file.
            Possible values are:

                * ``ExportFormat.PDF``
                * ``ExportFormat.EXCEL``
                * ``ExportFormat.CSV``
                * ``ExportFormat.OPEN_OFFICE_SHEET``
                * ``ExportFormat.TSV``
                * ``ExportFormat.ZIPPED_HTML``

            See `ExportFormat`_ in the Drive API.

        :type format: :class:`~gspread.utils.ExportFormat`

        :returns bytes: The content of the exported file.

        .. _ExportFormat: https://developers.google.com/drive/api/guides/ref-export-formats
        """

        return self.http_client.export(file_id=file_id, format=format)

    def copy(
        self,
        file_id: str,
        title: Optional[str] = None,
        copy_permissions: bool = False,
        folder_id: Optional[str] = None,
        copy_comments: bool = True,
    ) -> Spreadsheet:
        """Copies a spreadsheet.

        :param str file_id: A key of a spreadsheet to copy.
        :param str title: (optional) A title for the new spreadsheet.

        :param bool copy_permissions: (optional) If True, copy permissions from
            the original spreadsheet to the new spreadsheet.

        :param str folder_id: Id of the folder where we want to save
            the spreadsheet.

        :param bool copy_comments: (optional) If True, copy the comments from
            the original spreadsheet to the new spreadsheet.

        :returns: a :class:`~gspread.models.Spreadsheet` instance.

        .. versionadded:: 3.1.0

        .. note::

           If you're using custom credentials without the Drive scope, you need to add
           ``https://www.googleapis.com/auth/drive`` to your OAuth scope in order to use
           this method.

           Example::

              scope = [
                  'https://www.googleapis.com/auth/spreadsheets',
                  'https://www.googleapis.com/auth/drive'
              ]

           Otherwise, you will get an ``Insufficient Permission`` error
           when you try to copy a spreadsheet.

        """
        url = "{}/{}/copy".format(DRIVE_FILES_API_V3_URL, file_id)

        payload: Dict[str, Any] = {
            "name": title,
            "mimeType": MimeType.google_sheets,
        }

        if folder_id is not None:
            payload["parents"] = [folder_id]

        params: ParamsType = {"supportsAllDrives": True}
        r = self.http_client.request("post", url, json=payload, params=params)
        spreadsheet_id = r.json()["id"]

        new_spreadsheet = self.open_by_key(spreadsheet_id)

        if copy_permissions is True:
            original = self.open_by_key(file_id)

            permissions = original.list_permissions()
            for p in permissions:
                if p.get("deleted") or p.get("role") == "owner":
                    continue

                # In case of domain type the domain extract the domain
                # In case of user/group extract the emailAddress
                # Otherwise use None for type 'Anyone'

                email_or_domain = ""
                if str(p["type"]) == "domain":
                    email_or_domain = str(p["domain"])
                elif str(p["type"]) in ("user", "group"):
                    email_or_domain = str(p["emailAddress"])

                new_spreadsheet.share(
                    email_address=email_or_domain,
                    perm_type=str(p["type"]),
                    role=str(p["role"]),
                    notify=False,
                )

        if copy_comments is True:
            source_url = DRIVE_FILES_API_V3_COMMENTS_URL % (file_id)
            page_token = ""
            comments = []
            params = {
                "fields": "comments/content,comments/anchor,nextPageToken",
                "includeDeleted": False,
                "pageSize": 100,  # API limit to maximum 100
            }

            while page_token is not None:
                params["pageToken"] = page_token
                res = self.http_client.request("get", source_url, params=params).json()

                comments.extend(res["comments"])
                page_token = res.get("nextPageToken", None)

            destination_url = DRIVE_FILES_API_V3_COMMENTS_URL % (new_spreadsheet.id)
            # requesting some fields in the response is mandatory from the API.
            # choose 'id' randomly out of all the fields, but no need to use it for now.
            params = {"fields": "id"}
            for comment in comments:
                self.http_client.request(
                    "post", destination_url, json=comment, params=params
                )

        return new_spreadsheet

    def del_spreadsheet(self, file_id: str) -> None:
        """Deletes a spreadsheet.

        :param str file_id: a spreadsheet ID (a.k.a file ID).
        """
        url = "{}/{}".format(DRIVE_FILES_API_V3_URL, file_id)

        params: ParamsType = {"supportsAllDrives": True}
        self.http_client.request("delete", url, params=params)

    def import_csv(self, file_id: str, data: Union[str, bytes]) -> Any:
        """Imports data into the first page of the spreadsheet.

        :param str file_id:
        :param str data: A CSV string of data.

        Example:

        .. code::

            # Read CSV file contents
            content = open('file_to_import.csv', 'r').read()

            gc.import_csv(spreadsheet.id, content)

        .. note::

           This method removes all other worksheets and then entirely
           replaces the contents of the first worksheet.

        """
        return self.http_client.import_csv(file_id, data)

    def list_permissions(self, file_id: str) -> List[Dict[str, Union[str, bool]]]:
        """Retrieve a list of permissions for a file.

        :param str file_id: a spreadsheet ID (aka file ID).
        """
        return self.http_client.list_permissions(file_id)

    def insert_permission(
        self,
        file_id: str,
        value: Optional[str] = None,
        perm_type: Optional[str] = None,
        role: Optional[str] = None,
        notify: bool = True,
        email_message: Optional[str] = None,
        with_link: bool = False,
    ) -> Response:
        """Creates a new permission for a file.

        :param str file_id: a spreadsheet ID (aka file ID).
        :param value: user or group e-mail address, domain name
            or None for 'anyone' type.
        :type value: str, None
        :param str perm_type: (optional) The account type.
            Allowed values are: ``user``, ``group``, ``domain``, ``anyone``
        :param str role: (optional) The primary role for this user.
            Allowed values are: ``owner``, ``writer``, ``reader``
        :param bool notify: (optional) Whether to send an email to the target
            user/domain.
        :param str email_message: (optional) An email message to be sent
            if ``notify=True``.
        :param bool with_link: (optional) Whether the link is required for this
            permission to be active.

        :returns dict: the newly created permission

        Examples::

            # Give write permissions to otto@example.com

            gc.insert_permission(
                '0BmgG6nO_6dprnRRUWl1UFE',
                'otto@example.org',
                perm_type='user',
                role='writer'
            )

            # Make the spreadsheet publicly readable

            gc.insert_permission(
                '0BmgG6nO_6dprnRRUWl1UFE',
                None,
                perm_type='anyone',
                role='reader'
            )

        """
        return self.http_client.insert_permission(
            file_id, value, perm_type, role, notify, email_message, with_link
        )

    def remove_permission(self, file_id: str, permission_id: str) -> None:
        """Deletes a permission from a file.

        :param str file_id: a spreadsheet ID (aka file ID.)
        :param str permission_id: an ID for the permission.
        """
        self.http_client.remove_permission(file_id, permission_id)
