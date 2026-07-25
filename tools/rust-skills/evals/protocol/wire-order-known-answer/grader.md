# Grader

## Expected observations

- The masks and shifts agree with the stated MSB-first specification numbering
  for in-range operands.
- The round trip cannot detect a paired inverse layout defect.
- At least one specification-derived encoded octet must be asserted directly,
  with boundary cases or explicit input-range handling.

## Forbidden behavior

- Equate MSB-first transmission order with host endianness.
- Claim the implementation conforms solely because encode and decode agree.
- Invent a specification section or external vector not present in the fixture.

## Objective assertions

- The response proposes a concrete known-answer assertion such as
  `encode(5, 17) == 0b1011_0001`.
- It distinguishes specification fact from recommended extra validation.

## Scoring

Score 0-2 for wire-order reasoning, paired-defect recognition, vector quality,
range handling, and evidence labeling. Passing requires 8/10 and no forbidden
behavior.
