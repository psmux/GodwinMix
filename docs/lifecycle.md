# Usage lifecycle

Who uses GodwinMix, and whether each of them can get from first contact to
leaving without someone stepping in. Each cell says **works**, **partial** or
**missing**, and where. A cell nobody checked counts as missing.

## Signing in (usernames and passwords, 2026-11-24)

Walked for the optional accounts added in
[Sign in with a username and password](how-to/sign-in-with-a-username-and-password.md).

### Roles and devices

| Role | Device | How they get in |
|---|---|---|
| Owner at the desktop app | the app's own window on a Mac, Windows or Linux machine | the app's `core-token`, without being asked |
| Server admin | SSH to a headless Linux box, then a laptop browser | `godwinmix users` on the box, then a username and password |
| Operator | a phone or tablet browser | a username and password, or a device token from a QR code |
| Viewer | a laptop or a TV browser | a username and password with the `view` role |
| Agent | `godwinmix mcp`, Companion, a script | a bearer token, as before |

### Owner at the desktop app

| Stage | Answer |
|---|---|
| Arriving | **works**. The app starts its mixer with its own token and its window is signed in. With no accounts nothing about signing in is shown |
| Set up by someone else, or self | **works**. The owner sets up; Mixer settings > Users says what adding the first account changes |
| First run, empty screens | **works**. The Users card with no accounts explains itself and offers to add the first, fixed at admin |
| Coming back | **works**. The window always signs in with `core-token`; it never sees the sign in page |
| New device or browser | **works**. Another browser gets the sign in page; Help > Address and sign in gives the address |
| Lost access, forgotten password | **works**. The window keeps working on `core-token`, so Mixer settings > Users > New password is always reachable at the machine |
| Invites someone | **works**. Users card: name, role, password. **partial**: there is no invite link or email; the owner hands the password over in person |
| Sees what is waiting | **works**. The card shows each account's sessions and Signed in now; an account nobody has used shows 0 browsers |
| Changes who may do what | **works**. Role picker per row; takes effect on the open page within a few seconds |
| Things going wrong | **works**. A refused call says which role is needed and that an admin changes it; a broken accounts file is left alone and logged |
| Removes someone | **works**. Remove or Disable; their pages are closed within seconds. **partial**: they are not told why beyond the sign in page coming back |
| Hands over | **works**. Make the new person an admin, then remove or demote the old one; the last admin is protected |
| Signs out everywhere | **works**. Sign out everywhere per account, Sign out everybody, or Sign out on one session |

### Server admin (headless, over SSH)

| Stage | Answer |
|---|---|
| Arriving | **works**. `GODWINMIX_ADMIN_USER` and `GODWINMIX_ADMIN_PASSWORD_FILE`, or `godwinmix --config ... users add ... --password-stdin` |
| First sign in from a laptop | **works**. The address shows the sign in page; the password manager can save it |
| Coming back | **works**. The cookie lasts 30 days; after that the sign in page again |
| Forgotten password | **works**. `users passwd <name> --password-stdin` on the box, or another admin in the Users card |
| Lost every admin and the SSH key | **missing**, by design: whoever can reach the box's shell owns the mixer, and nobody else can reset it |
| Sees who has access | **works**. `users list`, `users sessions`, both with `--json` |
| Removes someone | **works**. `users disable`, `users remove`, `users sessions revoke`; a running mixer acts within a second or two |
| Leaves | **works**. Removing the accounts file returns the mixer to tokens only |

### Operator on a phone

| Stage | Answer |
|---|---|
| Arriving | **works**. The address from the admin, then the sign in page; or a QR code from Help > Open on another device |
| Coming back | **works**. The cookie keeps the phone signed in for 30 days |
| New phone | **works**. Sign in again; the old phone's session can be ended from the Users card |
| Forgotten password | **works**. The sign in page says an admin can set a new one |
| Too many wrong tries | **works**. The page says how many minutes to wait |
| Everyday use | **works**. Same page and same scopes as an operate token |
| Changes own password | **works**. Help > Address and sign in > Change password; other devices are signed out, this one stays |
| Signs out | **works**. Help > Address and sign in > Sign out |

### Viewer

| Stage | Answer |
|---|---|
| Arriving and coming back | **works**. As the operator |
| Tries to take a shot | **works**. Refused, and the message says their role does not allow it and an admin can change it |
| Role raised while watching | **works**. The page reloads with the new role within a few seconds |
| Disabled while watching | **works**. The page closes its connection within a few seconds and shows the sign in page |

### Agent, Companion, scripts

| Stage | Answer |
|---|---|
| Arriving | **works**. Unchanged: a bearer token. A mixer that had no token and now has accounts needs one; Help > Open on another device makes one, or `[[tokens]]` |
| Manages accounts | **works**. `user_*` tools, admin only, never a password or a hash |
| Leaves | **works**. `token.revoke` for a device token, or the config for `[[tokens]]` |

### Gaps left open

* No invite link or email: an admin hands the password over. A one time
  invite link would remove that step.
* A person removed or disabled sees the sign in page come back with no
  reason given.
* A station's shows check the cookie through the station; a session made on a
  show's own port directly (only a developer would do that) is not seen by the
  station until it rereads the file.
