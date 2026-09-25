"""Workspace resource capability contracts.

The v2 SDK grows a new resource API every quarter, and the ``lep`` CLI keeps
re-implementing the same per-resource command flows (list, inspect, watch
readiness, tail logs, stop/restart) with only the route names differing. To
converge on one generic resource driver, every resource API class must satisfy
one unified capability interface, so the operation families shared by the
platform are extracted here as formal contracts:

- :class:`ResourceHealthAPI` - workload health inspection (readiness/
  termination detail).
- :class:`ResourceRuntimeAPI` - runtime visibility (replica list, live logs,
  event history).
- :class:`ResourceLifecycleAPI` - state control (stop / restart).
- :class:`ResourceModelCatalogAPI` - the fine-tune model catalog.

:class:`WorkspaceResourceAPI` merges those families into the single standard
workspace-resource surface. Resources are grouped into workload resources
(:class:`WorkloadResourceAPI`) and configuration resources
(:class:`ConfigurationResourceAPI`); each group ships total, non-failing
defaults for the families that do not apply, so a generic driver never needs
a per-resource special case and no resource API has to raise ``NotImplementedError``.

Implementation note: because the merged interface is total over every family,
a new resource API merely declares ``WorkloadResourceAPI`` or
``ConfigurationResourceAPI`` as its base to become drivable everywhere.
"""

import abc
from typing import Iterator, List, Union

from .api_resource import APIResourse
from .types.deployment import LeptonDeployment
from .types.events import LeptonEvent
from .types.finetune import FineTuneModelInfo, TrainerInfo
from .types.readiness import ReadinessIssue
from .types.replica import Replica
from .types.termination import DeploymentTerminations


class ResourceHealthAPI(abc.ABC):
    """Workload health inspection: the readiness / termination detail views."""

    @abc.abstractmethod
    def get_readiness(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> ReadinessIssue:
        """Return the readiness issues reported for the given resource."""

    @abc.abstractmethod
    def get_termination(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> DeploymentTerminations:
        """Return the termination records reported for the given resource."""


class ResourceRuntimeAPI(abc.ABC):
    """Runtime visibility: replica list, live log tailing, event history."""

    @abc.abstractmethod
    def get_replicas(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> List[Replica]:
        """List the replicas currently backing the given resource."""

    @abc.abstractmethod
    def get_log(
        self,
        name_or_resource: Union[str, LeptonDeployment],
        replica: Union[str, Replica],
        timeout=None,
    ) -> Iterator[str]:
        """Stream the live log of the given resource's replica."""

    @abc.abstractmethod
    def get_events(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> List[LeptonEvent]:
        """List the platform events recorded for the given resource."""


class ResourceLifecycleAPI(abc.ABC):
    """State control: stop (scale to zero) and restart."""

    @abc.abstractmethod
    def stop(self, name_or_resource: Union[str, LeptonDeployment]) -> LeptonDeployment:
        """Stop the given resource."""

    @abc.abstractmethod
    def restart(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> LeptonDeployment:
        """Restart the given resource."""


class ResourceModelCatalogAPI(abc.ABC):
    """The fine-tune model catalog: supported models and trainers."""

    @abc.abstractmethod
    def list_supported_models(self) -> List[FineTuneModelInfo]:
        """List the models supported for fine-tuning."""

    @abc.abstractmethod
    def list_trainers(self, default_only: bool = True) -> List[TrainerInfo]:
        """List the fine-tune trainers."""


class WorkspaceResourceAPI(
    ResourceHealthAPI,
    ResourceRuntimeAPI,
    ResourceLifecycleAPI,
    ResourceModelCatalogAPI,
    APIResourse,
):
    """The unified workspace-resource capability surface.

    A workspace resource that satisfies this contract can be enumerated,
    inspected, driven, and catalogued identically: the driver does not need to
    know which resource it is operating on. Every resource API class in the
    SDK derives from :class:`WorkloadResourceAPI` or
    :class:`ConfigurationResourceAPI` below, so nothing in this class is
    implemented here - the subclasses own the actual operations.
    """


class WorkloadResourceAPI(WorkspaceResourceAPI):
    """Profile for resources with a runtime (deployments, pods, jobs).

    Workload resources exercise the health, runtime-visibility, and
    lifecycle families directly; only the fine-tune catalog is irrelevant
    for them, so a neutral empty catalog is provided here once for the
    whole family instead of an exception in the driver.
    """

    def list_supported_models(self) -> List[FineTuneModelInfo]:
        """No fine-tune catalog for generic workload resources.

        Reporting an empty catalog keeps generic catalog listings working
        over every workload resource; only fine-tune exposes real entries.
        """
        return []

    def list_trainers(self, default_only: bool = True) -> List[TrainerInfo]:
        """No trainer catalog for generic workload resources; see
        :meth:`list_supported_models`."""
        return []


class ConfigurationResourceAPI(WorkspaceResourceAPI):
    """Profile for configuration resources (secrets, ingresses, templates...).

    Configuration resources have no replicas, logs, readiness, or lifecycle,
    but they still ride the unified contract so drivers treat every
    workspace resource uniformly. This profile implements the whole
    operational surface with defined, non-failing defaults, so subclasses
    only add the resource's real management operations.
    """

    def get_readiness(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> ReadinessIssue:
        """Configuration resources report an empty readiness map.

        They are admitted or rejected at write time, so there is no runtime
        readiness to inspect; an empty map lets generic readiness views
        render without a special case.
        """
        return ReadinessIssue()

    def get_termination(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> DeploymentTerminations:
        """Configuration resources keep no termination records."""
        return DeploymentTerminations()

    def get_replicas(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> List[Replica]:
        """Configuration resources are never backed by replicas."""
        return []

    def get_log(
        self,
        name_or_resource: Union[str, LeptonDeployment],
        replica: Union[str, Replica] = None,
        timeout=None,
    ) -> Iterator[str]:
        """Configuration resources produce no runtime logs.

        Yields nothing so the shared log tailing loop terminates
        immediately instead of erroring on these resources.
        """
        return iter([])

    def get_events(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> List[LeptonEvent]:
        """Configuration resources record no workload events."""
        return []

    def stop(self, name_or_resource: Union[str, LeptonDeployment]) -> LeptonDeployment:
        """Configuration resources have no runtime to stop.

        Ignoring the request keeps a generic lifecycle driver simple; the
        accepted contract here is that stopping a configuration resource
        leaves it unchanged.
        """
        return None  # type: ignore[return-value]

    def restart(
        self, name_or_resource: Union[str, LeptonDeployment]
    ) -> LeptonDeployment:
        """Configuration resources have no runtime to restart.

        Ignoring the request keeps a generic lifecycle driver simple;
        restarting a configuration resource leaves it unchanged.
        """
        return None  # type: ignore[return-value]

    def list_supported_models(self) -> List[FineTuneModelInfo]:
        """Configuration resources expose no fine-tune model catalog."""
        return []

    def list_trainers(self, default_only: bool = True) -> List[TrainerInfo]:
        """Configuration resources expose no fine-tune trainer catalog."""
        return []
