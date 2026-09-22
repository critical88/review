# Refactoring task: working with site files feels mechanical everywhere

## Where this comes from

This Flask app manages Nginx site configuration files: the domains page lists
them with their modification times, and the domain API creates, updates,
deletes, and enables/disables them. A while back we introduced a small record
type for "one site configuration file" and reorganized the backend behind the
domains page and the `/domains` and `/domain/<name>` routes around it.

The reorganization made the code easier to navigate, but working with the
record is consistently awkward. Almost every place that handles a site file
grabs the record and works over several of its fields right there: it looks at
the raw file name to decide whether the site matches a request or is enabled,
rebuilds the on-disk path from the directory and the name to read or rewrite
the file, pulls the timestamp out for the page, or splits the file name apart
to rename the underlying file when enabling or disabling. When we wanted to
adjust how the enabled state is encoded lately, we had to trace every one of
those places to find which ones re-derive it — that is exactly the kind of
sprint we wanted to be done with.

## What we want

Please clean this up so that all the data-coupled knowledge about a site file —
matching a domain name, where its file is, loading and storing its content, the
enabled/disabled convention, and whatever a viewer needs such as display name
and timestamp — is implemented in one place where the data lives, instead of
being re-derived by every caller. Callers should be able to ask for the
outcome they need. We leave the concrete design up to you; the backend is
small, so take the liberty to restructure it as long as the shape stays
coherent.

Scope: the site-configuration backend (`app/api`) — the site-file record, the
collaborators that scan, edit, and present site files, and the endpoint code
that wires them. The `/config/<name>` endpoints, the templates, and the app
wiring are not part of this.

## What must not change

Keep the current behavior exactly, both in the responses and on disk:

- Routes, status codes, and JSON response bodies of the domains overview and
  the create/update/delete/enable-disabling API stay as they are.
- The domains page still shows the same grouping of available vs. enabled
  sites, the same display names (including the further name split for
  disabled sites), and the same modification times from the file mtime.
- File handling stays identical: prefix-style matching on the file name, the
  trailing `.disabled` suffix, the `.conf` convention, and newly created sites
  starting disabled.
- In particular, applying create/update/delete/enable to a site produces the
  exact same files as today.
- The repository's test suite passes unchanged.

## Where to look first

The enable/disable handling and the code that builds the domains page are the
spots where the mechanical feel bothered us most — each works over the record
in place instead of getting an answer from it. They are orientation, not a
complete inventory.
