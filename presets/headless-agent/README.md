# The headless-agent preset

No screen: the mixer runs on a server, an agent directs it over MCP, and the only surface is the API.

## What it gives you

A channel that runs without anybody watching it: a web page on air, a standby page to fall back to, and an agent that decides between them over MCP. No UI is started at all.

## What you need

A server you can reach over SSH, one destination to publish to, and an MCP client (Claude Code, or anything that speaks MCP) pointed at this machine.

## Three steps

1. `gmx preset apply headless-agent`. Nobody is at this machine, so this one is applied from the command line on purpose.
2. Start the mixer with a control token and give that token to your agent.
3. Ask the agent for the mixer's state, then to take a source.

## When it does not work

**The agent cannot see the mixer.** Usually the token is wrong or the port is closed. `gmx ctl status --url http://your-host:8080 --token ...` from your own machine answers in one line.

**The page is blank on air.** Usually it loaded before the data did. The standby source is there for this: the agent takes it while the page settles.

**The stream stops overnight.** Usually look at the alerts in the session log rather than guessing: `gmx support-bundle` collects the log, the config and every pipeline's state into one file.
