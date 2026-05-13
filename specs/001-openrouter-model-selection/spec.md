# Feature Specification: OpenRouter Model Selection

**Feature Branch**: `001-openrouter-model-selection`
**Created**: 2026-05-13
**Status**: Draft
**Input**: User description: "claude has tried implementing openrouter provide and I am not sure it suceeded. the spec was that all the user does is provides their api key, then they can search and select their model that provider unlocks."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Set Up OpenRouter With One Key (Priority: P1)

A user chooses OpenRouter as their LLM provider, enters their OpenRouter key, validates it, and completes provider setup without needing to enter model-specific keys or credentials for the underlying model vendors.

**Why this priority**: This is the core value of the feature. OpenRouter is useful only if the user can provide one key and then access the models available through that provider.

**Independent Test**: Can be fully tested by selecting OpenRouter, entering a valid key, validating it, and confirming setup is complete with no additional provider credential prompts.

**Acceptance Scenarios**:

1. **Given** the user is in provider setup, **When** they select OpenRouter and enter a valid OpenRouter key, **Then** the product accepts the key and treats OpenRouter as ready for model selection.
2. **Given** the user has selected OpenRouter, **When** the product asks for credentials, **Then** it asks only for the OpenRouter key and does not ask for keys from Anthropic, OpenAI, Google, or other routed model vendors.
3. **Given** the user enters an invalid or expired OpenRouter key, **When** they validate it, **Then** the product clearly says the key cannot be used and does not mark setup complete.

---

### User Story 2 - Search Available OpenRouter Models (Priority: P2)

After providing a valid OpenRouter key, a user searches the OpenRouter model list and sees only models they can reasonably choose through that provider experience, with enough identifying information to distinguish similarly named models.

**Why this priority**: OpenRouter can expose a large catalog. Search is required for the user to find a specific model without scrolling through an impractical list.

**Independent Test**: Can be fully tested by validating a key, opening the model selector, searching for a known model name or provider name, and confirming matching choices appear.

**Acceptance Scenarios**:

1. **Given** the user has a valid OpenRouter key, **When** they open the model selector, **Then** the product loads and displays available OpenRouter model choices.
2. **Given** the model selector contains many models, **When** the user types a search term such as a vendor name, model family, or model id fragment, **Then** the visible list narrows to matching models.
3. **Given** the model list cannot be loaded, **When** the user opens the selector, **Then** the product shows a recoverable error and still offers a safe way to continue with known default choices or retry later.

---

### User Story 3 - Select and Use an OpenRouter Model (Priority: P3)

A user selects one OpenRouter model from the searchable list, saves that choice, and subsequent build-reasoning requests use that selected model until the user changes it.

**Why this priority**: Model selection is only valuable if it persists and drives the product behavior the user cares about.

**Independent Test**: Can be fully tested by selecting an OpenRouter model, leaving and returning to settings, confirming the selected model remains selected, and running an LLM-assisted action that uses that model.

**Acceptance Scenarios**:

1. **Given** the user has searched the OpenRouter model list, **When** they select a model, **Then** the chosen model is shown as the active OpenRouter model.
2. **Given** the user has selected an OpenRouter model, **When** they close and reopen the product settings, **Then** the same model remains selected.
3. **Given** OpenRouter is the active provider and a model has been selected, **When** the user runs a build-reasoning action, **Then** the product routes the action through OpenRouter using the selected model.

### Edge Cases

- The key is valid but the account has no usable credits or access for the selected model.
- The model catalog is temporarily unavailable or returns no results.
- A previously selected model is no longer available to the user's OpenRouter account.
- The user changes from OpenRouter to another provider and then back again.
- The user searches with different casing, spaces, partial ids, or provider names.
- The model list is large enough that displaying all choices at once would be difficult to navigate.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Users MUST be able to select OpenRouter as the active LLM provider.
- **FR-002**: Users MUST be able to enter and save exactly one OpenRouter credential for OpenRouter setup.
- **FR-003**: The product MUST NOT require separate credentials for the individual model vendors routed through OpenRouter.
- **FR-004**: The product MUST validate whether the entered OpenRouter credential can be used before marking OpenRouter setup complete.
- **FR-005**: The product MUST distinguish invalid credentials from usable credentials that have account, quota, billing, or model-access limitations.
- **FR-006**: Users MUST be able to load model choices for OpenRouter after providing a usable OpenRouter credential.
- **FR-007**: Users MUST be able to search OpenRouter model choices by model id, model display name, model family, or provider/vendor name where that information is available.
- **FR-008**: The model selector MUST remain usable when the OpenRouter model catalog is large.
- **FR-009**: Users MUST be able to select one OpenRouter model as the active model.
- **FR-010**: The product MUST persist the selected OpenRouter model and restore it in later sessions.
- **FR-011**: When OpenRouter is active, LLM-assisted build reasoning MUST use the selected OpenRouter model.
- **FR-012**: If no OpenRouter model has been selected yet, the product MUST provide a sensible default or clearly guide the user to select a model before running OpenRouter-backed actions.
- **FR-013**: If a selected model becomes unavailable, the product MUST notify the user and require a new valid selection rather than silently using a different model.
- **FR-014**: The product MUST provide clear recovery actions when key validation, catalog loading, search, selection, or model use fails.
- **FR-015**: The product MUST avoid exposing the user's full OpenRouter credential in normal settings, logs, status messages, or error displays.

### Key Entities

- **OpenRouter Credential**: The user's OpenRouter key and its validation state, including whether it is accepted, rejected, or accepted with account limitations.
- **OpenRouter Model**: A model choice available through OpenRouter, identified by a stable id and user-readable label, with optional vendor or model-family metadata when available.
- **OpenRouter Model Selection**: The user's currently selected OpenRouter model and whether it is still valid for the current credential.
- **Model Search Query**: The user's current filter text used to narrow the OpenRouter model list.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At least 95% of users with a valid OpenRouter key can complete OpenRouter setup without entering any other LLM provider credential.
- **SC-002**: Users can find and select a known OpenRouter model from a catalog of at least 200 models in under 30 seconds.
- **SC-003**: Search results update quickly enough that users perceive filtering as immediate for a catalog of at least 200 models.
- **SC-004**: After selecting an OpenRouter model, 100% of later OpenRouter-backed build-reasoning actions use that selected model unless the user changes it or the model is no longer available.
- **SC-005**: After restarting the product, 100% of saved OpenRouter model selections are restored when the same credential remains usable.
- **SC-006**: Users receive an actionable explanation for invalid keys, unavailable model catalogs, unavailable selected models, and account limitation failures.

## Assumptions

- Existing OpenRouter provider setup, credential storage, model listing, model selection, and active-model routing should be reused where verified. This feature is gap closure and verification, not a rebuild of working provider infrastructure.
- OpenRouter remains a separate selectable provider in the existing provider setup flow.
- A valid OpenRouter key is enough for the product to discover and use models that OpenRouter makes available to that account.
- The product may show known default OpenRouter models when live model discovery is unavailable, but defaults must be clearly distinguishable from confirmed account-available choices.
- The selected OpenRouter model is stored as a stable model id rather than only a display label.
- This feature does not require users to manage per-model pricing, routing preferences, or advanced OpenRouter account settings inside the product.
