from typing import Union, List, Iterator, Optional
import warnings

from .api_resource import APIResourse
from .resource_contracts import WorkloadResourceAPI
from .types.deployment import LeptonDeployment, LeptonDeploymentUserSpec
from .types.events import LeptonEvent
from .types.readiness import ReadinessIssue
from .types.replica import Replica
from .types.termination import DeploymentTerminations


class PodAPI(WorkloadResourceAPI):
    def _to_name(self, name_or_pod: Union[str, LeptonDeployment]) -> str:
        return (  # type: ignore
            name_or_pod if isinstance(name_or_pod, str) else name_or_pod.metadata.id_
        )

    def _sanity_check_pod_spec(self, spec: Optional[LeptonDeploymentUserSpec]):
        """
        Sanity checks a pod spec, raising an exception if it is invalid, removes
        fields that take no effect in pod spec, and returns the spec.
        """
        if spec is None:
            warnings.warn(
                "You have not specified a pod spec - is that intentional?",
                RuntimeWarning,
            )
            return None
        if not spec.is_pod:
            raise ValueError("The spec is not a pod spec.")
        if spec.allow_unauthenticated_access is not None:
            raise ValueError(
                "allow_unauthenticated_access applies only to endpoints and cannot"
                " be set on a pod spec."
            )
        if spec.resource_requirement:
            if spec.resource_requirement.min_replicas not in (None, 1):
                warnings.warn(
                    "min_replicas does not take effect in pod spec.", RuntimeWarning
                )
                spec.resource_requirement.min_replicas = 1
            if spec.resource_requirement.max_replicas not in (None, 1):
                warnings.warn(
                    "max_replicas does not take effect in pod spec.", RuntimeWarning
                )
                spec.resource_requirement.max_replicas = 1
        if spec.auto_scaler:
            warnings.warn(
                "Auto scaler does not take effect in pod spec.", RuntimeWarning
            )
            spec.auto_scaler = None
        if spec.api_tokens:
            warnings.warn("API tokens do not take effect in pod spec.", RuntimeWarning)
            spec.api_tokens = None
        # TODO: add other fields check if needed.
        return spec

    def list_all(self) -> List[LeptonDeployment]:
        response = self._get("/deployments")
        deployments = self.ensure_list(response, LeptonDeployment)
        return [d for d in deployments if d.spec.is_pod]

    def create(self, spec: LeptonDeployment):
        """
        Create a deployment with the given deployment spec.
        """
        spec.spec = self._sanity_check_pod_spec(spec.spec)
        response = self._post("/deployments", json=self.safe_json(spec))
        return self.ensure_ok(response)

    # Implementation note: this legacy pod API delegates through an internal
    # accessor that honors an explicitly injected deployment wrapper while
    # otherwise returning ``_deployment_legacy`` directly. It intentionally does
    # not read the dispatching ``client.deployment`` property: in a flag-on
    # workspace that property returns EndpointAPI, not the legacy API PodAPI was
    # built around. When the flag is on, ordinary callers reach DevPodAPI instead
    # of this class entirely.
    def get(self, name_or_pod: Union[str, LeptonDeployment]) -> LeptonDeployment:
        return self._client._deployment_api_for_legacy_pod().get(name_or_pod)

    def update(
        self, name_or_deployment: Union[str, LeptonDeployment], spec: LeptonDeployment
    ) -> LeptonDeployment:
        raise RuntimeError(
            "Updating a pod is not supported. Updating a pod will cause all pod"
            " resources (including local storage) to be lost, and we strongly recommend"
            " you to be careful in doing so."
        )

    def delete(self, name_or_deployment: Union[str, LeptonDeployment]) -> bool:
        return self._client._deployment_api_for_legacy_pod().delete(name_or_deployment)

    def stop(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> LeptonDeployment:
        return self._client._deployment_api_for_legacy_pod().stop(name_or_deployment)

    def restart(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> LeptonDeployment:
        return self._client._deployment_api_for_legacy_pod().restart(name_or_deployment)

    def get_readiness(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> ReadinessIssue:
        return self._client._deployment_api_for_legacy_pod().get_readiness(
            name_or_deployment
        )

    def get_termination(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> DeploymentTerminations:
        return self._client._deployment_api_for_legacy_pod().get_termination(
            name_or_deployment
        )

    # Implementation note: pod does not support get_replicas.

    def get_log(
        self,
        name_or_deployment: Union[str, LeptonDeployment],
        timeout: Optional[int] = None,
    ) -> Iterator[str]:
        """
        Gets the log of the given deployment's specified replica. The log is streamed
        in chunks until timeout is reached. If timeout is not specified, the log will be
        streamed indefinitely, although you should not rely on this behavior as connections
        can be dropped when streamed for a long time.
        """
        deployment_api = self._client._deployment_api_for_legacy_pod()
        replicas = deployment_api.get_replicas(name_or_deployment)
        if len(replicas) != 1:
            raise RuntimeError(
                "You encountered a programming error: number of replicas should be 1"
                " for pods."
            )
        return deployment_api.get_log(name_or_deployment, replicas[0], timeout)

    # Implementation note: pod does not support get_replicas via a dedicated
    # route; a pod is always a single replica, so the unified replica view is
    # satisfied with an empty list rather than an unsupported error.
    def get_replicas(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> List[Replica]:
        """Pods do not expose a replica list; return an empty replica set.

        A generic replica view finds pods through the deployment API, so
        walking pods here simply finds no replica rows to render.
        """
        return []

    # Implementation note: pod events are served by the deployment API's
    # event history, not by a pod-scoped route; an empty list keeps the
    # unified event view total over pods.
    def get_events(
        self, name_or_deployment: Union[str, LeptonDeployment]
    ) -> List[LeptonEvent]:
        """Pods keep no separate event history; return an empty event list."""
        return []

    # TODO: implement api for the various metrics, but for now we will simply ask users
    # to view the metrics from the web portal.
