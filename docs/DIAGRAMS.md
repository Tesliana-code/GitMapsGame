# Diagram gallery

These diagrams intentionally depict a **synthetic educational model**, not TriageAI deployment topology.

## 1. World map

```mermaid
flowchart LR
  S[Source evidence] --> J[Judgment boundary]
  J --> H[Decision history]
  H --> R[Release assurance]
  classDef node fill:#d6f7ec,color:#0d2825,stroke:#168a7b
  class S,J,H,R node
```

## 2. Unlock conditions

```mermaid
stateDiagram-v2
  [*] --> UNKNOWN
  UNKNOWN --> DISCOVERED: Evidence defines crossing
  DISCOVERED --> CLAIMED: Scope and owner declared
  CLAIMED --> PROVEN: Reviewed evidence accepted
  PROVEN --> UNLOCKED: Dependency is satisfied
  CLAIMED --> DISCOVERED: Claim abandoned
```

## 3. Independent release gate

```mermaid
flowchart TD
  A[Feature candidate] --> B{CI passed?}
  B -- No --> STOP1[Hold: CI failure]
  B -- Yes --> C{Independent approval?}
  C -- No --> STOP2[Hold: no authorization]
  C -- Yes --> D{Exact revision matches?}
  D -- No --> STOP3[Hold: stale approval]
  D -- Yes --> E[Mock release accepted]
  E --> F[Mock verification]
```

## 4. Contributors and authority

```mermaid
sequenceDiagram
  participant P as Player / agent
  participant M as Map
  participant R as Reviewer
  participant G as Separate release authority
  P->>M: Claim crossing
  P->>R: Submit code and evidence
  R-->>M: Proof accepted
  M-->>P: Unlock dependent work
  Note over M,G: Unlock does not authorize a release
  G-->>G: Validate release policy independently
```
