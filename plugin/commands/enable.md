# Enable Superego

Enable superego for this project/session.

## Check project state first:

1. **If `.superego/` doesn't exist**: Offer to initialize - "Superego isn't set up for this project yet. Would you like to initialize it?" Then follow the init flow (check for binary, install if needed, run `sg init`).

2. **If `.superego/` exists but was disabled**: Re-enable evaluation. Tell user: "Superego feedback is now enabled. Evaluation will resume."

3. **If already enabled**: Confirm it's active: "Superego is already enabled and monitoring this session."

Be concise. Check state and act accordingly.
