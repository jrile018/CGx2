"""Local deterministic package, not external-package acceptance evidence."""
with open(__file__ + '.initialized', 'a', encoding='utf-8') as marker:
    marker.write('I')


def echo(value):
    return value


def keywords(value, *, suffix):
    return value + suffix


def wrong():
    return True


def fail():
    raise ValueError('fixture exploded: 雪')


def large():
    return 'x' * 1048577


def deep():
    result = 1
    for _ in range(18):
        result = [result]
    return result


def cycle():
    result = []
    result.append(result)
    return result


def nonstring_key():
    return {1: 'blocked'}


class Hostile:
    def __str__(self):
        raise AssertionError('must not stringify a returned object')

    def __reduce__(self):
        raise AssertionError('must not pickle a returned object')

    def __del__(self):
        with open(__file__ + '.released', 'a', encoding='utf-8') as marker:
            marker.write('R')


def hostile():
    return Hostile()


def unlisted():
    import cgx2_not_locked
    return cgx2_not_locked.value


def dependency(value):
    from pinned_dep import transform
    return transform(value)


def startup():
    import sys
    return (bool(sys.flags.isolated), 'site' in sys.modules, bool(sys.flags.no_site))
