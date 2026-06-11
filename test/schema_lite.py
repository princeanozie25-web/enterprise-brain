"""schema_lite: a deliberately tiny JSON-Schema validator (P-9 substrate).

Stdlib only. This is NOT a general JSON-Schema implementation: it supports
exactly the subset that the checked-in schemas under test/schemas/ use, and
it fails loudly on anything outside that subset so a typo in a schema can
never pass silently.

Supported keywords
------------------
  type                  one of "object", "array", "string", "integer",
                        "number", "boolean", "null" -- or a list of those
                        (the instance must match at least one)
  properties            mapping of property name -> subschema
  required              list of property names that must be present
  additionalProperties  boolean only; false rejects properties that are not
                        declared under "properties" (true / absent allows)
  items                 a single subschema applied to every array element
  enum                  list of permitted values
  const                 exactly one permitted value
  minimum / maximum     inclusive numeric bounds
  minItems / maxItems   inclusive bounds on array length
  minLength             minimum string length
  pattern               regular expression matched with re.search(), i.e.
                        unanchored per JSON Schema; schemas anchor with
                        ^...$ where they mean the whole string

"$schema", "$id", "title" and "description" are accepted as inert
annotations. Any other keyword raises SchemaError when its schema node is
visited.

Semantics (matching JSON Schema wherever the subset overlaps it)
----------------------------------------------------------------
  * A keyword constrains only instances of its own kind: "minimum" ignores
    strings, "minLength" ignores numbers, "items" ignores objects, and so
    on. Schemas pair these with "type" to turn mismatches into failures.
  * Python bools are NOT integers/numbers here, even though
    isinstance(True, int) holds: {"type": "integer"} rejects true, and
    enum/const never conflate true/false with 1/0.
  * "integer" requires an actual JSON integer (Python int); 5.0 does not
    count. The fixture generator only ever writes real ints.

validate(instance, schema, path="$") raises SchemaError on the FIRST
violation found, in a fixed deterministic order (type, enum, const,
numeric bounds, string bounds, array bounds/items, object
required/additionalProperties/properties), with the JSON-path of the
offending value (e.g. "$.documents[3].acl_refs[0].kind") in the message.
It returns None when the instance conforms.
"""

from __future__ import annotations

import re

__all__ = ["SchemaError", "validate"]


class SchemaError(Exception):
    """An instance violated the schema, or the schema itself stepped outside
    the supported subset.

    Attributes:
        path:   JSON-path of the offending value, e.g. "$.people[0].site".
        reason: human-readable explanation (str(error) is "{path}: {reason}").
    """

    def __init__(self, path: str, reason: str) -> None:
        super().__init__(f"{path}: {reason}")
        self.path = path
        self.reason = reason


# Exact-type predicates. type(...) checks are used for the numeric kinds
# (rather than isinstance) because bool subclasses int in Python while JSON
# keeps booleans and numbers disjoint. json.loads only ever produces these
# exact types, so exact checks are safe and precise.
_TYPE_CHECKS = {
    "object": lambda v: isinstance(v, dict),
    "array": lambda v: isinstance(v, list),
    "string": lambda v: isinstance(v, str),
    "integer": lambda v: type(v) is int,
    "number": lambda v: type(v) is int or type(v) is float,
    "boolean": lambda v: type(v) is bool,
    "null": lambda v: v is None,
}

_ANNOTATION_KEYWORDS = frozenset({"$schema", "$id", "title", "description"})
_SUPPORTED_KEYWORDS = _ANNOTATION_KEYWORDS | frozenset(
    {
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "const",
        "minimum",
        "maximum",
        "minItems",
        "maxItems",
        "minLength",
        "pattern",
    }
)


def _is_number(value: object) -> bool:
    """JSON number check: int or float, never bool."""
    return type(value) is int or type(value) is float


def _json_equal(a: object, b: object) -> bool:
    """Equality for enum/const that never conflates bools with 0/1."""
    if isinstance(a, bool) != isinstance(b, bool):
        return False
    return a == b


def _type_name(value: object) -> str:
    """Best-effort JSON type name for error messages."""
    for name, check in _TYPE_CHECKS.items():
        if check(value):
            return name
    return type(value).__name__  # not reachable for json.loads output


def validate(instance: object, schema: dict, path: str = "$") -> None:
    """Validate ``instance`` against ``schema``.

    Raises SchemaError (message prefixed with the JSON-path of the offending
    value) at the first violation; returns None when the instance conforms.
    """
    if not isinstance(schema, dict):
        raise SchemaError(
            path, f"schema node must be an object, got {_type_name(schema)}"
        )

    for keyword in schema:
        if keyword not in _SUPPORTED_KEYWORDS:
            raise SchemaError(path, f"unsupported schema keyword {keyword!r}")

    # 1. type ---------------------------------------------------------------
    if "type" in schema:
        names = schema["type"]
        if isinstance(names, str):
            names = [names]
        for name in names:
            if name not in _TYPE_CHECKS:
                raise SchemaError(path, f"unknown type name {name!r} in schema")
        if not any(_TYPE_CHECKS[name](instance) for name in names):
            raise SchemaError(
                path,
                f"expected type {' or '.join(names)}, got {_type_name(instance)}",
            )

    # 2. enum / const ---------------------------------------------------------
    if "enum" in schema and not any(
        _json_equal(instance, option) for option in schema["enum"]
    ):
        raise SchemaError(
            path, f"value {instance!r} is not one of enum {schema['enum']!r}"
        )
    if "const" in schema and not _json_equal(instance, schema["const"]):
        raise SchemaError(
            path, f"expected const {schema['const']!r}, got {instance!r}"
        )

    # 3. numeric bounds (numbers only; bools never reach here) ----------------
    if _is_number(instance):
        if "minimum" in schema and instance < schema["minimum"]:
            raise SchemaError(
                path, f"{instance!r} is less than minimum {schema['minimum']!r}"
            )
        if "maximum" in schema and instance > schema["maximum"]:
            raise SchemaError(
                path, f"{instance!r} is greater than maximum {schema['maximum']!r}"
            )

    # 4. string bounds ---------------------------------------------------------
    if isinstance(instance, str):
        if "minLength" in schema and len(instance) < schema["minLength"]:
            raise SchemaError(
                path,
                f"string of length {len(instance)} is shorter than "
                f"minLength {schema['minLength']}",
            )
        if "pattern" in schema and re.search(schema["pattern"], instance) is None:
            raise SchemaError(
                path,
                f"string {instance!r} does not match pattern {schema['pattern']!r}",
            )

    # 5. arrays -----------------------------------------------------------------
    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            raise SchemaError(
                path,
                f"array of {len(instance)} item(s) is shorter than "
                f"minItems {schema['minItems']}",
            )
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            raise SchemaError(
                path,
                f"array of {len(instance)} item(s) is longer than "
                f"maxItems {schema['maxItems']}",
            )
        if "items" in schema:
            for index, element in enumerate(instance):
                validate(element, schema["items"], f"{path}[{index}]")

    # 6. objects ------------------------------------------------------------------
    if isinstance(instance, dict):
        declared = schema.get("properties", {})
        extra_policy = schema.get("additionalProperties", True)
        if not isinstance(extra_policy, bool):
            raise SchemaError(
                path, "additionalProperties must be a boolean in this subset"
            )
        for name in schema.get("required", ()):
            if name not in instance:
                raise SchemaError(path, f"missing required property {name!r}")
        if extra_policy is False:
            # dict iteration follows document/insertion order: deterministic.
            for name in instance:
                if name not in declared:
                    raise SchemaError(f"{path}.{name}", "additional property not allowed")
        for name, subschema in declared.items():
            if name in instance:
                validate(instance[name], subschema, f"{path}.{name}")

    return None
