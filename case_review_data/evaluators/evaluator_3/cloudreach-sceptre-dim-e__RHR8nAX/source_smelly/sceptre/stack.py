# -*- coding: utf-8 -*-

"""
sceptre.stack

This module implements a Stack class, which stores a Stack's data.

"""

import logging

from typing import List, Dict, Union, Any, Optional
from deprecation import deprecated

from sceptre import __version__
from sceptre.connection_manager import ConnectionManager
from sceptre.exceptions import InvalidConfigFileError
from sceptre.helpers import (
    get_external_stack_name,
    sceptreise_path,
    create_deprecated_alias_property,
)
from sceptre.hooks import Hook, HookProperty
from sceptre.resolvers import (
    ResolvableContainerProperty,
    ResolvableValueProperty,
    RecursiveResolve,
    PlaceholderType,
    Resolver,
)
from sceptre.template import Template


class Stack:
    """
    Stack stores information about a particular CloudFormation Stack.

    :param name: The name of the Stack.

    :param project_code: A code which is prepended to the Stack names\
            of all Stacks built by Sceptre.

    :param template_path: The relative path to the CloudFormation, Jinja2,
            or Python template to build the Stack from. If this is filled,
            `template_handler_config` should not be filled. This field has been deprecated since
            version 4.0.0 and will be removed eventually.

    :param template_handler_config: Configuration for a Template Handler that can resolve
            its arguments to a template string. Should contain the `type` property to specify
            the type of template handler to load. Conflicts with `template_path`.

    :param region: The AWS region to build Stacks in.

    :param template_bucket_name: The name of the S3 bucket the Template is uploaded to.

    :param template_key_prefix: A prefix to the key used to store templates uploaded to S3

    :param required_version: A PEP 440 compatible version specifier. If the Sceptre version does\
            not fall within the given version requirement it will abort.

    :param parameters: The keys must match up with the name of the parameter.\
            The value must be of the type as defined in the template.

    :param sceptre_user_data: Data passed into\
            `sceptre_handler(sceptre_user_data)` function in Python templates\
            or accessible under `sceptre_user_data` variable within Jinja2\
            templates.

    :param hooks: A list of arbitrary shell or python commands or scripts to\
            run.

    :param s3_details: Details used for uploading templates to S3.

    :param dependencies: The relative path to the Stack, including the file\
            extension of the Stack.

    :param cloudformation_service_role: The ARN of a CloudFormation Service Role that is assumed\
            by CloudFormation to create, update or delete resources.

    :param protected: Stack protection against execution.

    :param tags: CloudFormation Tags to be applied to the Stack.

    :param external_name: The real stack name used for CloudFormation

    :param notifications: SNS topic ARNs to publish Stack related events to.\
            A maximum of 5 ARNs can be specified per Stack.

    :param on_failure: This parameter describes the action taken by\
            CloudFormation when a Stack fails to create.

    :param disable_rollback: If True, cloudformation will not rollback on deployment failures

    :param iam_role: The ARN of a role for Sceptre to assume before interacting
            with the environment. If not supplied, Sceptre uses the user's AWS CLI
            credentials. This field has been deprecated since version 4.0.0 and will be removed
            eventually.

    :param sceptre_role: The ARN of a role for Sceptre to assume before interacting\
            with the environment. If not supplied, Sceptre uses the user's AWS CLI\
            credentials.

    :param iam_role_session_duration: The duration in seconds of the assumed IAM role session.
            This field has been deprecated since version 4.0.0 and will be removed eventually.

    :param sceptre_role_session_duration: The duration in seconds of the assumed IAM role session.

    :param profile: The name of the profile as defined in ~/.aws/config and\
            ~/.aws/credentials.

    :param stack_timeout: A timeout in minutes before considering the Stack\
            deployment as failed. After the specified timeout, the Stack will\
            be rolled back. Specifying zero, as well as omitting the field,\
            will result in no timeout. Supports only positive integer value.

    :param ignore: If True, this stack will be ignored during launches (but it can be explicitly
            deployed with create, update, and delete commands.

    :param obsolete: If True, this stack will operate the same as if ignore was set, but it will
            also be deleted if the prune command is invoked or the --prune option is used with the
            launch command.

    :param sceptre_role_session_duration: The session duration when Scetre assumes a role.\
           If not supplied, Sceptre uses default value (3600 seconds)

    :param stack_group_config: The StackGroup config for the Stack

    :param config: The complete config for the stack. Used by dump config.
    """

    parameters = ResolvableContainerProperty("parameters")
    sceptre_user_data = ResolvableContainerProperty(
        "sceptre_user_data", PlaceholderType.alphanum
    )
    notifications = ResolvableContainerProperty("notifications")
    tags = ResolvableContainerProperty("tags")
    # placeholder_override=None here means that if the template_bucket_name is a resolver,
    # placeholders have been enabled, and that stack hasn't been deployed yet, commands that would
    # otherwise attempt to upload the template (like validate) won't actually use the template bucket
    # and will act as if there was no template bucket set.
    s3_details = ResolvableContainerProperty("s3_details", PlaceholderType.none)
    template_handler_config = ResolvableContainerProperty(
        "template_handler_config", PlaceholderType.alphanum
    )

    template_bucket_name = ResolvableValueProperty(
        "template_bucket_name", PlaceholderType.none
    )
    # Similarly, the placeholder_override=None for sceptre_role means that actions that would otherwise
    # use the sceptre_role will act as if there was no iam role when the sceptre_role stack has not been
    # deployed for commands that allow placeholders (like validate).
    sceptre_role = ResolvableValueProperty("sceptre_role", PlaceholderType.none)
    cloudformation_service_role = ResolvableValueProperty("cloudformation_service_role")

    hooks = HookProperty("hooks")

    iam_role = create_deprecated_alias_property(
        "iam_role",
        "sceptre_role",
        deprecated_in="4.0.0",
        removed_in=None,
    )
    role_arn = create_deprecated_alias_property(
        "role_arn",
        "cloudformation_service_role",
        deprecated_in="4.0.0",
        removed_in=None,
    )
    sceptre_role_session_duration = None
    iam_role_session_duration = create_deprecated_alias_property(
        "iam_role_session_duration",
        "sceptre_role_session_duration",
        deprecated_in="4.0.0",
        removed_in=None,
    )

    def __init__(
        self,
        name: str,
        project_code: str,
        region: str,
        template_path: str = None,
        template_handler_config: dict = None,
        template_bucket_name: str = None,
        template_key_prefix: str = None,
        required_version: str = None,
        parameters: dict = None,
        sceptre_user_data: dict = None,
        hooks: Hook = None,
        s3_details: dict = None,
        sceptre_role: str = None,
        iam_role: str = None,
        dependencies: List["Stack"] = None,
        cloudformation_service_role: str = None,
        role_arn: str = None,
        protected: bool = False,
        tags: dict = None,
        external_name: str = None,
        notifications: List[str] = None,
        on_failure: str = None,
        disable_rollback=False,
        profile: str = None,
        stack_timeout: int = 0,
        sceptre_role_session_duration: Optional[int] = None,
        iam_role_session_duration: Optional[int] = None,
        ignore=False,
        obsolete=False,
        stack_group_config: dict = None,
        config: dict = None,
    ):
        self.logger = logging.getLogger(__name__)

        self.name = sceptreise_path(name)
        self.project_code = project_code
        self.region = region
        self.required_version = required_version
        self.external_name = external_name or get_external_stack_name(
            self.project_code, self.name
        )
        self.dependencies = dependencies or []
        self.protected = protected
        self.on_failure = on_failure

        # Coerce disable_rollback to a boolean, running the boolean-type guard
        # that used to live in a dedicated helper straight inside the
        # constructor so the validated value is stored directly.
        if not isinstance(disable_rollback, bool):
            raise InvalidConfigFileError(
                f"{self.name}: Value for disable_rollback must be a boolean, "
                f"not a {type(disable_rollback).__name__}"
            )
        self.disable_rollback = disable_rollback

        self.stack_group_config = stack_group_config or {}
        self.config = config or {}
        self.stack_timeout = stack_timeout
        self.profile = profile
        self.template_key_prefix = template_key_prefix

        # Resolve the sceptre role session duration, preferring the current
        # attribute name and falling back to the deprecated iam role alias.
        # The deprecated-alias fallback uses setattr so the
        # iam_role_session_duration property setter redirects the value onto
        # sceptre_role_session_duration.
        if sceptre_role_session_duration and iam_role_session_duration:
            raise InvalidConfigFileError(
                "Both 'sceptre_role_session_duration' and 'iam_role_session_duration' are set. "
                "You should only set a value for sceptre_role_session_duration because "
                "iam_role_session_duration is deprecated."
            )
        elif sceptre_role_session_duration:
            setattr(self, "sceptre_role_session_duration", sceptre_role_session_duration)
        elif iam_role_session_duration:
            setattr(self, "iam_role_session_duration", iam_role_session_duration)
        else:
            setattr(self, "sceptre_role_session_duration", sceptre_role_session_duration)

        # Coerce ignore and obsolete to booleans through the same boolean-type
        # guard that was folded in for disable_rollback.
        if not isinstance(ignore, bool):
            raise InvalidConfigFileError(
                f"{self.name}: Value for ignore must be a boolean, "
                f"not a {type(ignore).__name__}"
            )
        self.ignore = ignore
        if not isinstance(obsolete, bool):
            raise InvalidConfigFileError(
                f"{self.name}: Value for obsolete must be a boolean, "
                f"not a {type(obsolete).__name__}"
            )
        self.obsolete = obsolete

        self._template = None
        self._connection_manager = None

        # Resolvers and hooks need to be assigned last
        self.s3_details = s3_details

        # Resolve the sceptre role, preferring the sceptre_role attribute and
        # falling back to the deprecated iam_role alias.
        if sceptre_role and iam_role:
            raise InvalidConfigFileError(
                "Both 'sceptre_role' and 'iam_role' are set. "
                "You should only set a value for sceptre_role because iam_role is deprecated."
            )
        elif sceptre_role:
            setattr(self, "sceptre_role", sceptre_role)
        elif iam_role:
            setattr(self, "iam_role", iam_role)
        else:
            setattr(self, "sceptre_role", sceptre_role)

        self.tags = tags or {}

        # Resolve the CloudFormation service role, preferring the current
        # cloudformation_service_role attribute and falling back to role_arn.
        if cloudformation_service_role and role_arn:
            raise InvalidConfigFileError(
                "Both 'cloudformation_service_role' and 'role_arn' are set. "
                "You should only set a value for cloudformation_service_role because role_arn is deprecated."
            )
        elif cloudformation_service_role:
            setattr(self, "cloudformation_service_role", cloudformation_service_role)
        elif role_arn:
            setattr(self, "role_arn", role_arn)
        else:
            setattr(self, "cloudformation_service_role", cloudformation_service_role)

        self.template_bucket_name = template_bucket_name

        # Resolve the template handler configuration, preferring the 'template'
        # config key and falling back to the deprecated template_path alias. The
        # 'template' key is required, so a missing value raises rather than
        # storing an empty configuration.
        if template_handler_config and template_path:
            raise InvalidConfigFileError(
                "Both 'template' and 'template_path' are set. "
                "You should only set a value for template because template_path is deprecated."
            )
        elif template_handler_config:
            setattr(self, "template_handler_config", template_handler_config)
        elif template_path:
            setattr(self, "template_path", template_path)
        else:
            raise InvalidConfigFileError("template is a required Stack Config.")

        self.s3_details = s3_details

        # Cast CloudFormation parameters inline: every value is coerced to a
        # string (or a list of strings / resolver), the resulting shape is
        # validated against the allowed expression types, and the validated
        # mapping is stored as parameters. The casting and validation helpers
        # are nested here so the constructor owns the whole transformation.
        resolved_parameters = parameters or {}

        def _cast_value(value: Any) -> Union[str, List[Union[str, Resolver]], Resolver]:
            if isinstance(value, bool):
                return "true" if value else "false"
            elif isinstance(value, (int, float)):
                return str(value)
            elif isinstance(value, list):
                return [_cast_value(item) for item in value]
            elif isinstance(value, Resolver):
                return value
            return value

        def _is_valid_parameter(value: Any) -> bool:
            return (
                isinstance(value, str)
                or (
                    isinstance(value, list)
                    and all(
                        isinstance(item, str) or isinstance(item, Resolver)
                        for item in value
                    )
                )
                or isinstance(value, Resolver)
            )

        if not isinstance(resolved_parameters, dict):
            raise InvalidConfigFileError(
                f"{self.name}: parameters must be a dictionary of key-value pairs, "
                f"got {resolved_parameters}"
            )

        casted_parameters = {k: _cast_value(v) for k, v in resolved_parameters.items()}

        if not all(
            _is_valid_parameter(value) for value in casted_parameters.values()
        ):
            raise InvalidConfigFileError(
                f"{self.name}: Values for parameters must be strings, lists or resolvers, "
                f"got {casted_parameters}"
            )
        self.parameters = casted_parameters

        self.sceptre_user_data = sceptre_user_data or {}
        self.notifications = notifications or []

        self.hooks = hooks or {}

    def __repr__(self):
        return (
            "sceptre.stack.Stack("
            f"name='{self.name}', "
            f"project_code={self.project_code}, "
            f"template_handler_config={self.template_handler_config}, "
            f"region={self.region}, "
            f"template_bucket_name={self.template_bucket_name}, "
            f"template_key_prefix={self.template_key_prefix}, "
            f"required_version={self.required_version}, "
            f"sceptre_role={self.sceptre_role}, "
            f"sceptre_role_session_duration={self.sceptre_role_session_duration}, "
            f"profile={self.profile}, "
            f"sceptre_user_data={self.sceptre_user_data}, "
            f"parameters={self.parameters}, "
            f"hooks={self.hooks}, "
            f"s3_details={self.s3_details}, "
            f"dependencies={self.dependencies}, "
            f"cloudformation_service_role={self.cloudformation_service_role}, "
            f"protected={self.protected}, "
            f"tags={self.tags}, "
            f"external_name={self.external_name}, "
            f"notifications={self.notifications}, "
            f"on_failure={self.on_failure}, "
            f"disable_rollback={self.disable_rollback}, "
            f"stack_timeout={self.stack_timeout}, "
            f"stack_group_config={self.stack_group_config}, "
            f"ignore={self.ignore}, "
            f"obsolete={self.obsolete}"
            ")"
        )

    def __str__(self):
        return self.name

    def __hash__(self):
        return hash(str(self))

    @property
    def connection_manager(self) -> ConnectionManager:
        """Returns the ConnectionManager for the stack, creating it if it has not yet been created.

        :returns: ConnectionManager.
        """
        if self._connection_manager is None:
            cache_connection_manager = True
            try:
                sceptre_role = self.sceptre_role
            except RecursiveResolve:
                # This would be the case when sceptre_role is set with a resolver (especially stack_output)
                # that uses the stack's connection manager. This creates a temporary condition where
                # you need the iam role to get the iam role. To get around this, it will temporarily
                # use None as the sceptre_role but will re-attempt to resolve the value in future accesses.
                # Since the Stack Output resolver (the most likely culprit) uses the target stack's
                # sceptre_role rather than the current stack's one anyway, it actually doesn't matter,
                # since the stack defining that sceptre_role won't actually be using that sceptre_role.
                self.logger.debug(
                    "Resolving sceptre_role requires the Stack connection manager. Temporarily setting "
                    "the sceptre_role to None until it can be fully resolved."
                )
                sceptre_role = None
                cache_connection_manager = False

            connection_manager = ConnectionManager(
                self.region,
                self.profile,
                self.external_name,
                sceptre_role,
                self.sceptre_role_session_duration,
            )
            if cache_connection_manager:
                self._connection_manager = connection_manager
            else:  # Return early without caching the connection manager.
                return connection_manager

        return self._connection_manager

    @property
    def template(self):
        """
        Returns the CloudFormation Template used to create the Stack.

        :returns: The Stack's template.
        :rtype: Template
        """
        if self._template is None:
            self._template = Template(
                name=self.name,
                handler_config=self.template_handler_config,
                sceptre_user_data=self.sceptre_user_data,
                stack_group_config=self.stack_group_config,
                s3_details=self.s3_details,
                connection_manager=self.connection_manager,
            )
        return self._template

    @property
    @deprecated(
        deprecated_in="4.0.0",
        removed_in=None,
        current_version=__version__,
        details="Use the template Stack Config key instead.",
    )
    def template_path(self) -> str:
        """The path argument from the template_handler config. This field is deprecated as of v4.0.0
        and will be removed in v5.0.0.
        """
        return self.template_handler_config["path"]

    @template_path.setter
    @deprecated(
        deprecated_in="4.0.0",
        removed_in=None,
        current_version=__version__,
        details="Use the template Stack Config key instead.",
    )
    def template_path(self, value: str):
        self.template_handler_config = {"type": "file", "path": value}
