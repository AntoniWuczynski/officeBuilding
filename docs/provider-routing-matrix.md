# Provider routing: what works with what

This page lists every pairing that Office Building's provider routing knows about and says how far each one has been taken. A frontend is the command line tool an agent runs in. A connection is the account and route that serves its requests. The page was last brought up to date on 13 November 2026.

Three things to know first:

- **No OpenAI route has ever met a real upstream.** The machine this was built on had no OpenAI application programming interface (API) key and no registration with OpenAI. The OpenAI route is tested against a stand-in only, with replies written from OpenAI's published documentation.
- One pairing has carried a real request: Claude Code on a Claude subscription.
- Routing is off until you switch it on in Settings. With it off, every desk starts exactly as it did before.

## What each word means

| Word | Meaning |
| --- | --- |
| Checked by a live probe | A real request went through on the build machine. The note gives the date, the versions, and what was and was not seen. |
| Built, not checked | The code and its tests exist. It has never been run against any account. |
| Experimental, not checked | The same, on a route that translates between two providers' formats. It also needs the "Allow this experimental route" switch. |
| Unavailable | It cannot be used in this build. The note gives the reason. |

## The matrix

| Frontend | Claude subscription | Anthropic API key | OpenRouter | OpenAI API key | Sign in with ChatGPT |
| --- | --- | --- | --- | --- | --- |
| Claude Code | Checked by a live probe (note 1) | Built, not checked (note 2) | Built, not checked (note 3) | Experimental, not checked (note 4) | Unavailable (note 5) |
| Codex | Unavailable (note 6) | Unavailable (note 6) | Unavailable (note 6) | Unavailable (note 6) | Unavailable (note 6) |
| Gemini command line interface | Unavailable (note 7) | Unavailable (note 7) | Unavailable (note 7) | Unavailable (note 7) | Unavailable (note 7) |
| Antigravity | Unavailable (note 8) | Unavailable (note 8) | Unavailable (note 8) | Unavailable (note 8) | Unavailable (note 8) |

Only desks that the building itself starts in one of its own terminals are routed. The reviewer desk, the manager, chat desks that run on a tool's own server, and anything started outside the app are not.

## Notes

**Note 1. Claude Code on a Claude subscription.** The request goes to Anthropic untouched, on the desk's own Claude login. The building does not store that login and sends it to no other host.

- Checked on 13 November 2026 with Claude Code 2.1.323: one request from a headless session, on the smallest model tier and with no tools, went through the built router to Anthropic and came back complete.
- Checked on 12 November 2026 with the same version, through a plain listener and not the router: the Claude login survives a changed base address, a header added at launch arrives on the main agent's and a helper agent's requests, a path prefix in the address is kept, and a refusal with status 400 stops the client at once and shows its sentence.
- Not checked: a real used-up allowance (the shape the router looks for was replayed by the probe, not produced by Anthropic), an interactive session, a long task, a workflow, and compaction.
- **An open question on Anthropic's terms.** Anthropic's legal page says developers "may not collect, store, or intermediate Claude.ai credentials or session tokens". Whether a relay on your own Mac, inside your own app, passing your own unmodified client on to Anthropic, falls under that sentence is not settled by Anthropic's documentation. The question is open in HUMAN_DECISIONS.md, entry OB-211, which recommends written confirmation from Anthropic before the app goes to anyone else. This connection kind can be switched off in the code without touching the others.

**Note 2. Claude Code with an Anthropic API key.** The desk's own login is dropped, the key from the Keychain is sent, and only the model name and the effort are replaced in the request. Not checked: whether Anthropic's API takes a request shaped for a subscription login when it arrives with a key. A key that is not tied to one workspace is refused, because the building has no field for a workspace.

**Note 3. Claude Code through OpenRouter.** The request stays in Anthropic's format and goes to OpenRouter's endpoint for that format, with OpenRouter's own fallback between providers switched off. Not checked: whether any provider there accepts everything Claude Code sends, and where OpenRouter puts the reason for a refused request in that format. Until a real refusal has been seen, a used-up OpenRouter budget may not be recognised.

**Note 4. Claude Code with an OpenAI API key.** Each request is translated from Anthropic's Messages format to OpenAI's Responses format, and the reply is translated back. It needs both "May spend money" and "Allow this experimental route", and a model name you have typed for each tier. Nothing is filled in for you.

| What | On this route |
| --- | --- |
| Text, the system prompt, images, tool definitions, tool calls and tool results | Carried in full, in their order, with their identifiers |
| Hidden reasoning | Left out in both directions. The desk's route strip says so. OpenAI advises passing its reasoning back between tool calls, and this route cannot, so answers may be weaker than on OpenAI's own tools. |
| Anthropic's cache marks, the thinking settings and the user identifier meant for Anthropic | Left out on every request |
| A failed tool call | Carried, marked by one added line, because OpenAI's tool output has no error flag |
| Documents, tools the provider runs (web search, tool search), stop sequences, structured output, context management, a request that is not streamed | Refused. The refusal names what could not be carried, and nothing is sent. |
| Effort | The effort you mapped for that tier. An effort with no pair refuses the request. |

A desk that has already used a document or a tool the provider runs is not moved to this connection by itself when another runs out. You can switch it by hand. Claude Code is expected to drop context management and tool search by itself after the first refusal, as its documentation describes. A local probe on 2026-11-13 saw Claude Code 2.1.323 drop context management when a test listener replayed the router's refusal. Neither has been seen through the router itself.

Anthropic does not support running Claude Code on models that are not Claude's, through any gateway.

**Note 5. Sign in with ChatGPT.** Not built, and the settings do not offer it. What is missing:

1. OpenAI offers plan use to "open-source partners" and does not say how an app becomes one. The owner of this app has to settle that with OpenAI first.
2. Which client identifier the app may send depends on that answer, so none is in the code.
3. The sign-in must check a signed identity token, which needs a library this app does not have.
4. OpenAI's preview of this route takes requests in a different shape from the API key route, and its documentation does not give all of it.
5. None of it can be tested without an accepted app and an eligible account.
6. The sign-in button must use OpenAI's approved branding.

**Note 6. Codex.** Codex is not a routed frontend in this build, on any connection. Checked on 12 November 2026 with Codex 0.161.0, with every request answered by a local stand-in and no model called: Codex accepts a provider set for one process and sends Responses requests to it. No supported way was found for Codex to use a Claude subscription.

**Note 7. Gemini command line interface.** Not built and not probed. No connection kind for Google's API exists in this build.

**Note 8. Antigravity.** Not built and not probed. The research behind this feature found that its custom endpoint takes requests in Google's format, and that Google's published restrictions do not allow an Antigravity consumer login to be used by other software. Neither was checked for this build.

## What has not been checked anywhere

- A real allowance running out, on any connection.
- Any OpenAI reply, of any sort.
- A desk carrying on after a move from one provider to another in the real app.
- Two desks with different orders at the same time, a helper agent or a workflow through the router, and an app restart with routed desks running.
