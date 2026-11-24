# Sign in with a username and password

A mixer can give each person their own username and password instead of
handing round a token. It is optional. A mixer with no accounts works exactly
as before: the page asks for a token, or for nothing on a mixer that has none.

Once one account exists, the page shows a sign in page with a username and a
password, and the word token goes out of the way. Tokens keep working
underneath for agents, Bitfocus Companion, `gmx` and scripts.

## What a role allows

| Role | What the person can do |
|---|---|
| `view` (Watch only) | see the multiview, the programme and the state. Takes nothing |
| `operate` (Run the show) | takes, transitions, graphics, sources and outputs |
| `admin` (Admin) | everything, including Mixer settings and the users |

A role is a token scope underneath (`read`, `operate`, `admin`), so a viewer
who tries to take a shot is refused the same way a read only token is. The
first account has to be an admin.

## Add people from the page

Open **Mixer settings**. The **Users and sign in** card is at the top.

1. Type a username, pick a role and a password of at least 8 characters,
   then press **Add user**.
2. Give the person the mixer's address, their username and the password.

The card lists every account with its role and how many browsers are signed
in as it. For each one you can change the role, set a **New password**,
**Sign out everywhere**, **Disable** or **Remove**. Disabling or removing an
account refuses its next call at once and closes its open pages within a few
seconds, on every device, including a page that is only watching. Changing a
role takes effect the same way: the page reloads and comes back with the new
role. **Signed in now** lists each browser with its
address and device, and **Sign out** ends one.

You cannot disable, demote or remove the last admin while other accounts
remain. Make someone else an admin first.

If the mixer had no token at all, adding the first account closes it to
anyone who is not signed in. The page that added it signs in as that account
by itself, so you are not shut out.

## Sign in and out

A browser that opens the mixer's address gets the sign in page. The browser's
password manager can save the password. The sign in lasts 30 days on that
browser.

**Help > Address and sign in** shows who the page is signed in as, with
**Sign out** and **Change password**. Changing your own password signs out
every other browser that is signed in as you; the one you are on stays. The
page's token, when it has one, is under **Advanced** in the same dialog.

Ten wrong passwords from one address, or five against one username, in 15
minutes, and the mixer refuses further tries for the rest of that window. The
refusal never says whether the username exists.

## On a server with no browser

The `users` subcommand works on the accounts file beside the config, with the
mixer running or stopped. A running mixer notices a change within a second.
Pass the config the service uses, before `users`:

```sh
printf '%s\n' "$PASSWORD" | godwinmix --config /etc/godwinmix/godwinmix.toml users add sam --role admin --password-stdin
godwinmix --config /etc/godwinmix/godwinmix.toml users list
godwinmix --config /etc/godwinmix/godwinmix.toml users list --json
printf '%s\n' "$NEW" | godwinmix --config /etc/godwinmix/godwinmix.toml users passwd sam --password-stdin
godwinmix --config /etc/godwinmix/godwinmix.toml users role sam operate
godwinmix --config /etc/godwinmix/godwinmix.toml users disable sam
godwinmix --config /etc/godwinmix/godwinmix.toml users enable sam
godwinmix --config /etc/godwinmix/godwinmix.toml users remove sam
godwinmix --config /etc/godwinmix/godwinmix.toml users sessions
godwinmix --config /etc/godwinmix/godwinmix.toml users sessions list sam --json
godwinmix --config /etc/godwinmix/godwinmix.toml users sessions revoke sam
godwinmix --config /etc/godwinmix/godwinmix.toml users sessions revoke --session s-abc123
godwinmix --config /etc/godwinmix/godwinmix.toml users sessions revoke --all
```

| Subcommand | What it does |
|---|---|
| `users list [--json]` | every account: name, role, enabled or disabled, sessions |
| `users add <name> [--role view\|operate\|admin] [--confirm] (--password-stdin \| --password-file <path>)` | make an account. `--role` defaults to `operate`; the first account must be `admin`. `--confirm` makes destructive calls ask first |
| `users passwd <name> (--password-stdin \| --password-file <path>)` | set a new password and end every session of that account |
| `users role <name> <view\|operate\|admin>` | change the role |
| `users disable <name>` / `users enable <name>` | stop or allow sign in; disabling ends its sessions |
| `users remove <name>` | remove the account and end its sessions |
| `users sessions [list [<name>] [--json]]` | the live sessions |
| `users sessions revoke (<name> \| --session <id> \| --all)` | end sessions |

A password is read from the first line of standard input or of a file, never
from the command line, where the shell history and `ps` would keep it. The
command exits 0 on success and 1 with the reason on standard error otherwise.
A username is up to 32 lower case letters, digits, dots, dashes and
underscores, starting with a letter or a digit; capitals are folded to lower
case.

Run it as the user the service runs as (`sudo -u godwinmix godwinmix ...`)
when you can. Run as root, it gives the file to whoever owns the config's
folder, so the service can still read it.

## The first admin on an unattended install

An installer can set the first admin through the service's environment
instead of running the command:

```sh
# /etc/godwinmix/env, mode 0600
GODWINMIX_ADMIN_USER=admin
GODWINMIX_ADMIN_PASSWORD_FILE=/etc/godwinmix/admin-password
```

`/etc/godwinmix/admin-password` holds the password on its first line, mode
0600. When the mixer starts with no account at all, it makes that admin and
deletes the password file. When any account exists, both variables are
ignored, so leaving them in the file does nothing on later starts and never
puts back a password somebody has since changed. If only one of the two is
set, the mixer logs a warning and makes nobody.

## Forgotten passwords and getting back in

* An admin sets a new one in **Mixer settings > Users > New password**, and
  hands it over in person.
* On the mixer's machine, `godwinmix --config <file> users passwd <name>
  --password-stdin` does the same with no page at all.
* If every admin is locked out, the person at the machine is not. The desktop
  app keeps its own token in `core-token` in its data folder and its window
  signs in with it, so Mixer settings is always reachable there. A server's
  `GODWINMIX_TOKEN`, or a `[[tokens]]` entry with admin scope, signs a page in
  the same way through **Advanced** on the sign in page.
* Removing the accounts file (`godwinmix.users.toml` beside the config) and
  restarting takes the mixer back to tokens only.

## Agents, Companion and scripts

Nothing changes for them. A bearer token in `Authorization: Bearer`, or
`?token=` on a GET, works as before, and a mixer with accounts still accepts
every token in its config, its device tokens and its plugin tokens. An admin
agent can manage accounts with the `user_list`, `user_add`, `user_update`,
`user_reset_password`, `user_remove`, `user_sessions` and `user_sign_out`
tools (find them with `search_tools`); none of them ever returns a password
or its hash.

## Where it is kept and how it is protected

* Accounts and sessions are in `<config stem>.users.toml` beside the config,
  mode 0600. Passwords are argon2id hashes. A session is kept as the SHA-256
  of its secret. Nothing in the file signs anybody in.
* The session is an HttpOnly, SameSite=Strict cookie named after the port
  (`gmx_session_8080`), marked Secure when the page is on HTTPS. A request
  carrying it is refused the cookie when the browser says it came from another
  site or another origin.
* Under a station, the station signs people in and every show reads the same
  file.

The details of each route and method are in
[Configuration](../reference/configuration.md#accounts) and in
`protocol.md` under `user.*` and `account.*`.
