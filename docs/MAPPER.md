# Object mapping

Every Galactus DB driver ships an optional mapper: it turns nodes and
relationships into the application's own objects and back, and writes the
Cypher for the everyday operations. It is part of the driver package, adds no
dependency, and uses the driver's public API only. An application that wants
to write every statement itself can ignore it.

This file is the contract the seven mappers share. Names follow each
language's conventions; the behaviour is the same.

## Declaring types

A **node type** has:

- a **label** (by default the type's name);
- **properties**, each a field of the type and the property it is stored as
  (by default the field's name);
- optionally a **key**: the property that identifies one node of the type to
  the application. A graph id is the database's own and is not a key.

A **relationship type** has a **type name** and properties.

Label, type and property names are written into Cypher inside backticks. A
name that is empty or holds a backtick is refused when the type is declared.

## Converting

| Operation | Result |
|---|---|
| properties of an object | A map of every declared property. A field with no value is `null` |
| object from a node | The declared properties read from the node. A property the node lacks leaves the field at its empty value; properties that are not declared are ignored |
| object from a relationship | The same, from a relationship |

A value of the wrong kind for its field is a mapping error that names the type
and the field.

## Operations

A mapper is made from a driver and runs each operation as one statement, in
the driver's open transaction when there is one. Values are always sent as
parameters.

| Operation | Statement | Result |
|---|---|---|
| `create(object)` | ``CREATE (n:`Label`) SET n = $properties RETURN n`` | The object as stored |
| `save(object)` | ``MERGE (n:`Label` {`key`: $key}) SET n += $properties RETURN n`` | The object as stored. Needs a key |
| `find(Type, key)` | ``MATCH (n:`Label` {`key`: $key}) RETURN n LIMIT 1`` | The object, or nothing |
| `findAll(Type, filter, limit)` | ``MATCH (n:`Label`) WHERE n.`p` = $p … RETURN n`` | Every match. `filter` maps property names to the values they must equal. Ordered by key when the type has one |
| `delete(Type, key)` | ``MATCH (n:`Label` {`key`: $key}) DETACH DELETE n RETURN count(*)`` | Whether a node was deleted |
| `relate(from, relationship, to)` | ``MATCH (a:`A` {`ka`: $a}), (b:`B` {`kb`: $b}) MERGE (a)-[r:`TYPE`]->(b) SET r += $properties RETURN r`` | The relationship as stored. Both ends need a key and must exist |
| `related(from, RelationshipType, ToType)` | ``MATCH (a:`A` {`ka`: $a})-[r:`TYPE`]->(b:`B`) RETURN r, b`` | Pairs of relationship and node. Ordered by the far node's key when it has one |
| `query(Type, cypher, parameters)` | The statement given | The first column of each row as the type: a node, a relationship, or a map of properties |

- `save`, `find`, `delete`, `relate` and `related` refuse a type with no key
  and a key whose value is `null`.
- `relate` fails when either end is not found.
- A filter may only name declared properties.
- `query` is the way to everything else: write the Cypher, and map what comes
  back.

## The shared test

Each mapper is tested against a live server with the same story:

1. `Person` has key `id`, `name`, `email` stored as `email_address`, and an
   optional `age`. `Company` has key `name`. `WorksAt` has `since` and `role`.
2. `create` a person; `find` returns an equal one; the raw node has
   `email_address`.
3. `save` changes it; `save` of a new key creates it.
4. `find` of a missing key returns nothing.
5. `findAll` with a filter and with a limit.
6. `relate` a person to a company, then `related` reads it back.
7. `query` maps the nodes a hand-written statement returns.
8. `delete` returns true, then false.
9. A type with no key cannot be saved, and a name with a backtick cannot be
   declared.
