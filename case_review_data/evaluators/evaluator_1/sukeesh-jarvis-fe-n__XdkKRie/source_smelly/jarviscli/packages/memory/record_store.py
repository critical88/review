import json

from packages.memory.memory import Memory


class RecordStore:
    """
    Snapshot of one record collection in the jarvis key value memory.

    Tasks, todo entries, reminders, user tags and named routines were moved
    from ad hoc JSON keys to named record collections. Each collection keeps
    one snapshot of its records together with the id handed out next. The
    store loads and stores the snapshot; the records themselves are managed
    by the plugins.
    """

    def __init__(self, jarvis, item_key, id_key=None):
        self.jarvis = jarvis
        self.item_key = item_key
        self.id_key = id_key
        self.items = None
        self.next_id = None
        self.had_invalid_text = False

    def load(self):
        self.items = self._load_items()
        self.next_id = self._load_next_id()

    def save(self):
        if self.jarvis.get_data(self.item_key) is None:
            self.jarvis.add_data(self.item_key, self._record_text())
        else:
            self.jarvis.update_data(self.item_key, self._record_text())

        if self.id_key is not None and self.next_id is not None:
            if self.jarvis.get_data(self.id_key) is None:
                self.jarvis.add_data(self.id_key, self.next_id)
            else:
                self.jarvis.update_data(self.id_key, self.next_id)

    def _record_text(self):
        return self.items

    def _load_items(self):
        raise NotImplementedError()

    def _load_next_id(self):
        if self.id_key is None:
            return None

        return self.jarvis.get_data(self.id_key)


class TaskRecordStore(RecordStore):
    """
    Snapshot of the task list stored under "tasks_list". Tasks keep their
    own "tasks.json" storage, so the list is loaded from and stored to that
    file.
    """

    def __init__(self):
        super().__init__(None, "tasks_list")

    def load(self):
        m = Memory("tasks.json")
        self.items = m.get_data(self.item_key)
        if self.items is None:
            self.items = []

    def save(self):
        m = Memory("tasks.json")
        if m.get_data(self.item_key) is None:
            m.add_data(self.item_key, self.items)
        else:
            m.update_data(self.item_key, self.items)
        m.save()


class TagRecordStore(RecordStore):
    """
    Snapshot of the user tag list stored under "reminder_tags".
    """

    def __init__(self, jarvis):
        super().__init__(jarvis, "reminder_tags", "reminder_tags_next_id")

    def _load_items(self):
        value = self.jarvis.get_data(self.item_key)
        if value is None:
            value = []
            self.jarvis.add_data(self.item_key, value)

        return value


class JsonRecordStore(RecordStore):
    """
    Snapshot of a collection stored as encoded record text. The text is set
    to the empty collection on the first read and when it could not be
    decoded.
    """

    def __init__(self, jarvis, item_key, id_key=None):
        super().__init__(jarvis, item_key, id_key)

    def _record_text(self):
        return json.dumps(self.items)

    def _load_items(self):
        value = self.jarvis.get_data(self.item_key)
        if value is None:
            value = "[]"
            self.jarvis.add_data(self.item_key, value)

        try:
            return json.loads(value)
        except json.decoder.JSONDecodeError:
            self.had_invalid_text = True
            value = "[]"
            self.jarvis.update_data(self.item_key, value)
            return json.loads(value)


class TodoRecordStore(JsonRecordStore):
    """
    Snapshot of the todo entries stored under "todo". The ids are handed out
    together with "remind" from the shared "todo_next_id" counter.
    """

    def __init__(self, jarvis):
        super().__init__(jarvis, "todo", "todo_next_id")


class RemindRecordStore(JsonRecordStore):
    """
    Snapshot of the reminders stored under "remind". The ids are handed out
    together with "todo" from the shared "todo_next_id" counter.
    """

    def __init__(self, jarvis):
        super().__init__(jarvis, "remind", "todo_next_id")


class RoutineRecordStore(RecordStore):
    """
    Snapshot of the named routines stored under "routines". Each routine
    value is the list of commands to run.
    """

    def __init__(self, jarvis):
        super().__init__(jarvis, "routines")

    def _load_items(self):
        return self.jarvis.get_data(self.item_key)
