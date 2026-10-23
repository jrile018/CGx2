"""Pinned copied-value adapter: datetime instances never cross the boundary."""
from dateutil.parser import isoparse


def isoformat(text):
    return isoparse(text).isoformat()
