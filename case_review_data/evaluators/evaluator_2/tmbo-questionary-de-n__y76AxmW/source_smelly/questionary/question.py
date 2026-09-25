import sys
from typing import Any

import prompt_toolkit.patch_stdout
from prompt_toolkit import Application

from questionary import utils
from questionary.constants import DEFAULT_KBI_MESSAGE


class Question:
    """A question to be prompted.

    This is an internal class. Questions should be created using the
    predefined questions (e.g. text or password)."""

    application: "Application[Any]"
    should_skip_question: bool
    default: Any

    def __init__(self, application: "Application[Any]") -> None:
        self.application = application
        self.should_skip_question = False
        self.default = None

    async def ask_async(
        self, patch_stdout: bool = False, kbi_msg: str = DEFAULT_KBI_MESSAGE
    ) -> Any:
        """Ask the question using asyncio and return user response.

        Args:
            patch_stdout: Ensure that the prompt renders correctly if other threads
                          are printing to stdout.

            kbi_msg: The message to be printed on a keyboard interrupt.

        Returns:
            `Any`: The answer from the question.
        """

        try:
            sys.stdout.flush()
            return await self.unsafe_ask_async(patch_stdout)
        except KeyboardInterrupt:
            print("{}".format(kbi_msg))
            return None

    def ask(
        self, patch_stdout: bool = False, kbi_msg: str = DEFAULT_KBI_MESSAGE
    ) -> Any:
        """Ask the question synchronously and return user response.

        Args:
            patch_stdout: Ensure that the prompt renders correctly if other threads
                          are printing to stdout.

            kbi_msg: The message to be printed on a keyboard interrupt.

        Returns:
            `Any`: The answer from the question.
        """

        try:
            return self.unsafe_ask(patch_stdout)
        except KeyboardInterrupt:
            print("{}".format(kbi_msg))
            return None

    def unsafe_ask(self, patch_stdout: bool = False) -> Any:
        """Ask the question synchronously and return user response.

        Does not catch keyboard interrupts.

        Args:
            patch_stdout: Ensure that the prompt renders correctly if other threads
                          are printing to stdout.

        Returns:
            `Any`: The answer from the question.
        """

        if self.should_skip_question:
            return self.default

        if patch_stdout:
            with prompt_toolkit.patch_stdout.patch_stdout():
                return self.application.run()
        else:
            return self.application.run()

    def skip_if(self, condition: bool, default: Any = None) -> "Question":
        """Skip the question if flag is set and return the default instead.

        Args:
            condition: A conditional boolean value.
            default: The default value to return.

        Returns:
            :class:`Question`: `self`.
        """

        self.should_skip_question = condition
        self.default = default
        return self

    async def unsafe_ask_async(self, patch_stdout: bool = False) -> Any:
        """Ask the question using asyncio and return user response.

        Does not catch keyboard interrupts.

        Args:
            patch_stdout: Ensure that the prompt renders correctly if other threads
                          are printing to stdout.

        Returns:
            `Any`: The answer from the question.
        """

        if self.should_skip_question:
            return self.default

        if utils.LEGACY_EVENT_LOOP_COMPAT:
            # The prompt_toolkit 2 pipeline replaced the event loop of the
            # host application and returned a plain future which had to be
            # wrapped in an asyncio future before it could be awaited
            utils.use_prompt_toolkit_2_event_loop()
            return await self.application.run_async().to_asyncio_future()  # type: ignore[attr-defined]

        if not utils.ACTIVATED_ASYNC_MODE:
            await utils.activate_prompt_toolkit_async_mode()

        if patch_stdout:
            with prompt_toolkit.patch_stdout.patch_stdout():
                r = self.application.run_async()
        else:
            r = self.application.run_async()

        if utils.is_prompt_toolkit_3():
            return await r
        else:
            return await r.to_asyncio_future()  # type: ignore[attr-defined]
