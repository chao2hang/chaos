# Auth system rule: first account is admin

**Date:** 2026-07-18  
**Status:** Implemented

## Rule

After chaos is installed and the database has **no users**:

1. The **only** way to create an account is **Initial setup** (`POST /api/v1/auth/setup`).
2. That **first created account is the administrator** (`users.role = admin`).
3. Setup cannot run again once any user exists (`409 already_initialized`).
4. Future additional accounts (if multi-user is added) default to `role = user` unless explicitly promoted.

## Implementation

| Layer | Behavior |
|-------|----------|
| `POST /auth/setup` | Requires `count(users) == 0`; calls `create_admin_user` |
| `POST /auth/login` | Issues JWT with `role` from DB |
| SQLite | `users.role` (`admin` \| `user`); migration promotes earliest existing user to admin |
| JWT `Claims.role` | Present on new tokens; used by `AuthUser::is_admin()` |

## Non-goals (this change)

- Multiple admins UI
- Invite flows
- Password reset

## Operator notes

- Forgotten admin password: delete the user row (or empty `users`) and re-run setup, **or** set a new hash offline — there is no recovery UI in MVP.
