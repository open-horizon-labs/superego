# Remove Superego

Remove superego from this project (inverse of init).

1. Check if `.superego/` exists - if not, tell user "Superego isn't initialized in this project."

2. **Ask for confirmation**: "This will delete the .superego/ directory including your custom prompt and configuration. Continue?"

3. If confirmed, remove the `.superego/` directory:
   ```bash
   rm -rf .superego/
   ```

4. Confirm: "Superego removed from this project. The plugin remains installed for other projects."

**Note**: This only removes the project configuration. The `sg` binary and plugin remain installed system-wide.
