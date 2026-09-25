from typing import Union, List, Optional

from .api_resource import APIResourse
from .resource_contracts import WorkloadResourceAPI
from leptonai.api.v2.types.job import LeptonJobQueryMode
from .types.deployment import LeptonDeployment
from .types.events import LeptonEvent
from .types.finetune import (
    LeptonFineTuneJob,
    FineTuneModelInfo,
    TrainerInfo,
)
from .types.readiness import ReadinessIssue
from .types.replica import Replica
from .types.termination import DeploymentTerminations


class FineTuneAPI(WorkloadResourceAPI):
    def _to_id(self, name_or_job: Union[str, LeptonFineTuneJob]) -> str:
        return (  # type: ignore
            name_or_job if isinstance(name_or_job, str) else name_or_job.metadata.id_
        )

    def list_all(
        self,
        *,
        job_query_mode: str = LeptonJobQueryMode.AliveOnly.value,
        q: Optional[str] = None,
        query: Optional[str] = None,
        status: Optional[List[str]] = None,
        node_groups: Optional[List[str]] = None,
        page: Optional[int] = None,
        page_size: Optional[int] = None,
        created_by: Optional[str] = None,
    ) -> List[LeptonFineTuneJob]:
        """
        List fine-tune jobs with optional server-side filtering.
        """
        params_base = {"job_query_mode": job_query_mode}
        if q:
            params_base["q"] = q
        if query:
            params_base["query"] = query
        if status:
            params_base["status"] = status
        if node_groups:
            params_base["node_groups"] = node_groups
        if created_by:
            params_base["created_by"] = created_by

        # If user explicitly specifies page or page_size, do single request
        if page is not None or page_size is not None:
            if page is not None:
                params_base["page"] = page
            if page_size is not None:
                params_base["page_size"] = page_size
            response = self._get("/finetune/jobs", params=params_base)
            return self.ensure_list(
                response, LeptonFineTuneJob, list_key="finetune_jobs"
            )

        # Otherwise auto-paginate until empty result set
        results: List[LeptonFineTuneJob] = []
        current_page = 1
        while True:
            params = dict(params_base)
            params["page"] = current_page
            params["page_size"] = 500
            response = self._get("/finetune/jobs", params=params)
            items = self.ensure_list(
                response, LeptonFineTuneJob, list_key="finetune_jobs"
            )
            if not items:
                break
            results.extend(items)
            current_page += 1
        return results

    def create(self, spec: LeptonFineTuneJob) -> LeptonFineTuneJob:
        response = self._post("/finetune/jobs", json=self.safe_json(spec))
        return self.ensure_type(response, LeptonFineTuneJob)

    def get(
        self,
        id_or_job: Union[str, LeptonFineTuneJob],
        *,
        job_query_mode: str = LeptonJobQueryMode.AliveOnly.value,
    ) -> LeptonFineTuneJob:
        response = self._get(
            f"/finetune/jobs/{self._to_id(id_or_job)}",
            params={"job_query_mode": job_query_mode} if job_query_mode else None,
        )
        return self.ensure_type(response, LeptonFineTuneJob)

    def update(
        self, name_or_job: Union[str, LeptonFineTuneJob], spec: LeptonFineTuneJob
    ) -> bool:
        response = self._patch(
            f"/finetune/jobs/{self._to_id(name_or_job)}", json=self.safe_json(spec)
        )
        return self.ensure_ok(response)

    def delete(
        self,
        name_or_job: Union[str, LeptonFineTuneJob],
        *,
        job_query_mode: str = LeptonJobQueryMode.AliveOnly.value,
    ) -> bool:
        response = self._delete(
            f"/finetune/jobs/{self._to_id(name_or_job)}",
            params={"job_query_mode": job_query_mode} if job_query_mode else None,
        )
        return self.ensure_ok(response)

    def list_supported_models(self) -> List[FineTuneModelInfo]:
        response = self._get("/finetune/supported-models")
        return self.ensure_list(response, FineTuneModelInfo)

    def list_trainers(self, default_only: bool = True) -> List[TrainerInfo]:
        response = self._get(
            "/finetune/trainers", params={"default_only": str(default_only).lower()}
        )
        return self.ensure_list(response, TrainerInfo)

    # Implementation note: fine-tune jobs ride the unified workload contract
    # so a generic driver can walk them alongside other resources, but the v2
    # fine-tune surface does not expose the deployment-style inspection
    # routes. The capability views below are therefore empty over fine-tune
    # jobs rather than unsupported, so shared dashboards keep rendering.
    def get_readiness(
        self, name_or_job: Union[str, LeptonFineTuneJob]
    ) -> ReadinessIssue:
        """Fine-tune jobs expose no per-replica readiness detail.

        Progress is reported on the job status; the deployment-style
        readiness map is empty over fine-tune jobs.
        """
        return ReadinessIssue()

    def get_termination(
        self, name_or_job: Union[str, LeptonFineTuneJob]
    ) -> DeploymentTerminations:
        """Fine-tune jobs expose no deployment-style termination records.

        Completion is recorded on the job status; the unified termination
        view is empty over fine-tune jobs.
        """
        return DeploymentTerminations()

    def get_replicas(
        self, name_or_job: Union[str, LeptonFineTuneJob]
    ) -> List[Replica]:
        """Fine-tune jobs do not expose a replica list.

        The unified replica view simply renders no replica rows for
        fine-tune jobs.
        """
        return []

    def get_log(
        self,
        name_or_job: Union[str, LeptonFineTuneJob],
        replica: Union[str, Replica] = None,
        timeout: Optional[int] = None,
    ):
        """Fine-tune job logs are not streamable through the v2 API yet.

        Yields nothing so a generic log tailing loop terminates immediately
        over fine-tune jobs instead of failing.
        """
        return iter([])

    def get_events(
        self, name_or_job: Union[str, LeptonFineTuneJob]
    ) -> List[LeptonEvent]:
        """Fine-tune jobs record no readable event history; return none."""
        return []

    # Implementation note: fine-tune jobs are submitted and left to finish;
    # there is no stop/restart route, so the lifecycle family is satisfied
    # with an explicit refusal.
    def stop(self, name_or_job: Union[str, LeptonFineTuneJob]) -> LeptonDeployment:
        """Fine-tune jobs cannot be stopped once submitted.

        A fine-tune job runs to completion; if it must be abandoned, delete
        it and submit a corrected one.
        """
        raise RuntimeError(
            "A fine-tune job cannot be stopped once submitted. Fine-tune jobs"
            " run to completion; delete the job if it must be interrupted."
        )

    def restart(self, name_or_job: Union[str, LeptonFineTuneJob]) -> LeptonDeployment:
        """Fine-tune jobs cannot be restarted; submit a new job instead."""
        raise RuntimeError(
            "A fine-tune job cannot be restarted. To rerun the same training"
            " spec, create a new fine-tune job."
        )
