# Feature Specification: Choya readable chat

**Feature Branch**: `006-choya-readable-chat`

**Created**: 2026-09-08

**Status**: Draft

**Input**: User description: in-game test of the Sprint 1 and Sprint 2 DLL on 2026-09-08. Opening a published build card hid the optimized build behind an identical-looking tab; a follow-up question made the published card vanish; Choya's replies are one wall of text cut off with three dots; a scoring question waited 4 min 37 s, failed on the model, and the fallback ran the optimizer for the wrong profession instead of answering the question. Scope agreed: never clip a reply, readable replies, colour-coded build tabs, persistent provider cards, a fallback that matches the request, and a status line while waiting with one log line per round.

## Problem

Choya can now produce a build and score one, but the player cannot read what it says, cannot find the build they just made after opening another, loses a card by asking a question, and, when the model fails, gets an answer to a question they did not ask. None of this is simulator or model work. It is the chat and comparison surface not keeping up with what the back end already delivers.

## Clarifications

### Session 2026-09-08

- Q: Is the prompt boundary owned by the latency work released so Choya can be asked for the reply shape? → A: Yes. Choya is prompted to be concise and to the point: facts, not long-winded prose, laid out with bullet points, bold, colour, italic and underline where each fits. Formatting is proper, never decorative.
- Q: On a slow model, keep the fixed 150 s limit or wait open-ended with a Cancel button? → A: Neither alone. The player cannot know whether to wait or cancel unless they can see what Choya is doing, so: a live thinking bubble that expands on click and shows the model's current output (its reasoning where the provider exposes it, the streamed answer or the tool activity otherwise), an open-ended wait with a Stop button, and a Retry button that nudges the model to continue. A player who sees nothing arriving can conclude it is stuck and press Stop; that is the only time they should have to.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A reply is shown whole and can be read (Priority: P1)

As a player, I want Choya's reply shown in full, short and to the point, laid out with bullets and emphasis instead of prose, so that I can read what it recommends in seconds instead of skimming a block and giving up.

**Why this priority**: Every other feature of Choya is worthless if the reply is unreadable or cut off. Today a reply is silently truncated after a fixed length and the rest is discarded.

**Independent Test**: Ask Choya for a build with a rotation. The bubble shows the whole reply, with the build's key terms visibly emphasised, list items on their own lines, the rotation as an arrow sequence, and links underlined and clickable. Nothing ends in three dots.

**Acceptance Scenarios**:

1. **Given** a reply longer than the old cut-off, **When** it is shown, **Then** every character of it is visible by scrolling and nothing is replaced by three dots.
2. **Given** a reply that lists items, **When** it is shown, **Then** each item is on its own line with a bullet.
3. **Given** a reply that describes a rotation, **When** it is shown, **Then** the skills read as a sequence joined by arrows, in order.
4. **Given** a reply that names a skill, trait or upgrade that the game data knows, **When** it is shown, **Then** that name is emphasised (bold or a theme colour) so it stands out from the prose.
5. **Given** a reply that contains a web address, **When** it is shown, **Then** the address is underlined and opens in the browser when clicked.
6. **Given** a reply, **When** it is shown, **Then** its text wraps to the bubble width and the bubble grows with it; the chat scrolls, the text never overflows the panel.
7. **Given** a build request, **When** Choya answers, **Then** the reply is facts in bullet points with at most two sentences of prose in a row; the build, the why and the rotation are each their own short block.
8. **Given** a reply, **When** it is shown, **Then** bold, colour, italic and underline each appear only where they carry meaning (names, warnings, asides, links), never across whole paragraphs.

---

### User Story 2 - The build tabs say whose build each is (Priority: P2)

As a player, I want the comparison tabs to tell me at a glance which build is the one I am wearing, which one the optimizer or Choya made, and which ones came from a build site, so that opening a published build never loses the one I just made.

**Why this priority**: The optimized build was still there in today's test; the player could not see it. That is a lost result from the player's point of view.

**Independent Test**: Run Optimize, then open a published build card. Two tabs are visible, each tinted by its kind, the optimized one is still selectable, and the published one names its site.

**Acceptance Scenarios**:

1. **Given** a current build, an optimized build and one or more published builds, **When** the strip is shown, **Then** each tab carries the tint of its kind (current, optimized, published) derived from the active theme, and the selected tab of each kind is highlighted in that kind's colour.
2. **Given** a published build tab, **When** it is shown, **Then** its label carries the site name.
3. **Given** an optimized build already in the strip, **When** a published card is opened, **Then** the optimized tab stays and the published tab is added beside it.
4. **Given** builds already in the strip, **When** a saved build is loaded, **Then** it is added as a tab rather than replacing the strip.
5. **Given** any theme preset or a custom theme, **When** the tints are derived, **Then** they remain distinguishable from each other and from the panel background.

---

### User Story 3 - Published cards stay until the next build (Priority: P3)

As a player, I want the published build cards Choya showed me to stay in the conversation while I ask follow-up questions, so that asking a question does not erase what I was about to compare.

**Why this priority**: Directly observed today. A single follow-up message removed the card.

**Independent Test**: Ask for a build, see the published card, ask "score that exact build". The card is still there after the answer.

**Acceptance Scenarios**:

1. **Given** published cards shown for a build request, **When** the player sends a question that does not ask for a new build, **Then** the cards remain.
2. **Given** published cards shown, **When** the player asks for a new build, **Then** the cards are replaced by the ones for the new build.
3. **Given** the chat is cleared, **When** the player looks at the conversation, **Then** the cards are gone with it.

---

### User Story 4 - When the model fails, the fallback answers the question asked (Priority: P4)

As a player, when the model does not answer, I want the fallback to match what I asked: a scoring question gets the referee's verdict on the build we were discussing, and a build request gets the optimizer's build for the profession the conversation is about, so that a failure never answers a different question.

**Why this priority**: Today a scoring question about a Reaper produced a Ritualist build from the optimizer. It looks like an answer and is not one.

**Independent Test**: With a model that will not answer, ask Choya to score the plated build. The reply is the referee's verdict on that build, not an optimizer run.

**Acceptance Scenarios**:

1. **Given** a plated build in the conversation and a scoring or explanation question, **When** the model fails, **Then** the reply carries the referee's verdict for that build (viable, gates, score, what was not simulated) and says the model did not answer.
2. **Given** a build request naming a profession or specialisation, **When** the model fails, **Then** the optimizer's build is for that profession and specialisation, not for the character's equipped one.
3. **Given** a message that is neither a build request nor about a plated build, **When** the model fails, **Then** the reply says so plainly and offers the two things the player can do next; it does not run the optimizer.

---

### User Story 5 - The player can see Choya thinking, stop it, or nudge it (Priority: P5)

As a player, I want to see what Choya is doing while I wait, the way a coding assistant shows its thinking, so that I can tell a slow model from a stuck one and decide for myself whether to keep waiting, stop, or ask it to continue.

**Why this priority**: Today's failure could not be diagnosed by the player or from the log, and a fixed limit would only have replaced a long wait with a guess about whether a longer one would have worked.

**Independent Test**: Send a request on a slow model. A thinking bubble appears with the step name and elapsed seconds; clicking it expands the model's live output; a Stop button ends the request; after a stop or a failure a Retry button asks the model to continue; the addon log has one line per step with its duration and outcome.

**Acceptance Scenarios**:

1. **Given** a request in flight, **When** the thinking bubble is shown collapsed, **Then** it names the current step (looking things up, scoring the build, writing the answer) and the seconds spent in it, updating live.
2. **Given** the thinking bubble, **When** the player clicks it, **Then** it expands to the model's current output: its reasoning where the provider exposes it, otherwise the answer as it streams in, otherwise the tool calls made so far with their results; the newest text is visible without scrolling.
3. **Given** no output has arrived for a stated interval, **When** the bubble is shown, **Then** it says so ("nothing for 45 s") so the player can judge a stall.
4. **Given** a request in flight, **When** the player presses Stop, **Then** the request ends within a second, the partial output stays visible in the conversation, and nothing else is started.
5. **Given** a stopped or failed request, **When** the player presses Retry, **Then** Choya is asked to continue from where it left off, with the partial output as context, rather than starting the whole request over.
6. **Given** a request that finished, failed or was stopped, **When** the addon log is read, **Then** each step has one line with its name, duration and outcome, and a failed step names the error the player saw.
7. **Given** a request on a model that never answers, **When** the player does nothing, **Then** the wait continues open-ended with the stall indicator counting up; no fixed limit ends it for them.

---

### Edge Cases

- A reply that is only a chat code, or only a link: shown whole, the link clickable.
- A reply that contains markup the game overlay cannot draw (tables, images): rendered as plain text, never dropped.
- Two published builds from the same site: two tabs, both labelled with the site name and distinguished by their build name.
- A theme where the optimized and published tints would be nearly equal: the derivation keeps a minimum contrast between kinds.
- The strip holds more tabs than fit: it scrolls or wraps; no tab is hidden without a way to reach it.
- A fallback for a scoring question when nothing has been plated yet: the reply says there is no build to score.
- A follow-up that is ambiguous between a new build and a question: treated as a question, the cards stay.
- A provider that exposes neither reasoning nor a stream: the expanded bubble shows the tool calls and the step timer only, and says the model's text arrives all at once.
- Retry pressed after a Stop that left no partial output: the request is sent again from the start and the bubble says so.
- Stop pressed while a tool (build scoring) is running: the tool finishes or is abandoned within a second; its result is not shown as an answer.


## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The chat MUST show every reply in full; no reply text is discarded or replaced by an ellipsis at any length.
- **FR-002**: The chat MUST lay out a reply with wrapped paragraphs, bulleted lists, arrow-joined rotation sequences, emphasised names of skills, traits and upgrades the game data knows, coloured warnings, italic asides, and underlined clickable web addresses.
- **FR-002a**: Choya MUST be asked, in its prompt, for concise fact-first replies in that shape: bullet points over prose, at most two consecutive sentences of prose, no filler. The prompt files are open to this sprint for that purpose only.
- **FR-003**: The comparison tab strip MUST show three visually distinct kinds of tab, current, optimized and published, tinted from the active theme, with the selected tab highlighted in its kind's colour, and MUST label published tabs with the site name.
- **FR-004**: Opening a published build or loading a saved build MUST add a tab; neither MUST remove existing tabs.
- **FR-005**: Published build cards MUST persist across follow-up messages and MUST be replaced only by a new build request or a chat clear.
- **FR-006**: When the model fails to answer, the fallback MUST be chosen by the kind of request: the referee's verdict for a scoring or explanation question about a plated build, the optimizer's build for the requested profession and specialisation for a build request, and a plain statement with next steps otherwise.
- **FR-007**: The thinking bubble MUST name the current step and show its elapsed seconds live, and the addon MUST log one line per step with name, duration and outcome.
- **FR-008**: The thinking bubble MUST expand on click to the model's live output (reasoning where exposed, else the streaming answer, else tool activity), keeping the newest text visible, and MUST state how long it has been since anything arrived.
- **FR-008a**: A request MUST wait open-ended; the player MUST have a Stop button that ends it within a second and keeps the partial output, and a Retry button that asks the model to continue from that output.
- **FR-009**: Existing chat, plating and scoring behaviour MUST be unchanged apart from the above; no scoring constant, gate or simulator rule moves.
- **FR-010**: All new player-facing text MUST be present in every supported locale.

### Key Entities

- **Reply**: the model's or fallback's text for one request, shown whole, with a layout derived from its structure.
- **Build tab**: one entry in the comparison strip with a kind (current, optimized, published), a label, and for published ones a site name.
- **Published card**: a build-site result attached to a build request, living until the next build request or a clear.
- **Request kind**: build request, question about a plated build, or other; drives the fallback.
- **Step**: one stage of a request (lookup round, build scoring, closing answer) with a start, duration and outcome.
- **Thinking bubble**: the in-flight view of a request: collapsed step and timer, expanded live output, stall interval, Stop and Retry.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero replies end in an ellipsis inserted by the addon; a 3,000-character reply is fully readable by scrolling.
- **SC-002**: In a reply with a rotation and a list, a reader can point to the rotation sequence and each list item within 5 seconds; a build reply has no run of prose longer than two sentences.
- **SC-003**: After Optimize followed by opening a published card, the optimized build is reachable in one click, and 100 percent of testers can say which tab is which without reading the labels.
- **SC-004**: A follow-up question after a plated build leaves every published card in place.
- **SC-005**: With the model unavailable, a scoring question about a plated build returns that build's verdict, never an optimizer run for another profession.
- **SC-006**: Every request leaves one log line per step; a failed request can be attributed to a step and a duration from the log alone.
- **SC-008**: On a request that stalls, a player can tell within 10 seconds of looking at the bubble that nothing is arriving, and Stop ends it within 1 second; Retry after a partial answer produces a continuation, not a restart.
- **SC-007**: Twelve locales carry every new string, and the locale parity test passes.

## Assumptions

- This sprint touches the addon's chat and comparison surfaces, their locale strings, and the reply-shape instruction in the prompts; the simulator, referee and search are untouched.
- Rendering runs inside the game overlay's own text drawing; rich layout means the subset the overlay can draw (wrapped text, bullets, coloured or bold spans, underlines, arrows), not HTML.
- Tab tints are derived from the theme palette the way other derived colours already are, so custom themes get them without new settings.
- The fallback's request-kind detection reuses the intent the chat already extracts for the model; ambiguous messages count as questions.
- The 150 s closing limit is replaced by the open-ended wait with Stop; the two lookup rounds from the latency work stay. Whether a provider exposes reasoning is a per-provider fact the plan records; the bubble degrades to the stream or the tool activity where it does not.
- The same release gate as Sprints 1 and 2 applies: commits stay local until the latency DLL is accepted in-game, then the addon ships with a patch bump after the player's test.
