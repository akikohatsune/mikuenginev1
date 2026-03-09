# Stockfish Techniques — Reference for mikuengine

Techniques to add incrementally. Check off as implemented.

## Medium Priority (Phase 3)


- [x] **Futility w/ correction history** — margin adjusted by `abs(correctionValue) / 174665`
- [x] **cutoffCnt LMR boost** — `+256 + 1024*(cnt>2) + 1024*allNode` based on child cutoffs
- [x] **Fail-low counter bonus** — complex bonus based on statScore, depth, moveCount
- [x] **Non-pawn correction** — White/Black non-pawn material keys
- [x] **Continuation correction** — `continuationCorrectionHistory[piece][to]` for ply-2 and ply-4
