# Examples

[Back to the README](../README.md)

Three tasks an agent carries out with Thoric, from the request to the answer. Each step names the tools of [the MCP guide](MCP.md), which describes every argument; the JSON shows only the arguments that matter. The animation at the top of the README is the same kind of work, recorded in a real installation: an agent opened a terminal with `desktop_input`, wrote and ran a script, waited for its window with `wait_for_condition`, looked at it with `screenshot`, changed the script and handed the result back with `save_screenshot`, while `start_recording` kept the frames.

The first and second tasks use `/mcp`, the administrator's endpoint; the third works through the MCP endpoint of one container, as an agent given only that desktop would.

## Build an image from a template and start desktops from it

The request: "Set up three desktops with Chrome, one for each of my test agents."

1. `get_settings` returns the installation's `version` and `templates_url`, the address of the template list. `list_images` shows whether an image built from the template is already there: its `template_ids` name the templates it came from.
2. The agent fetches the list with its own HTTP client, since the manager never fetches a template. It picks `chrome-mcp`, Google Chrome with Playwright MCP, checks that its `requires` is not newer than `version`, and fetches `chrome-mcp.json` from the same folder.
3. `build_image` with that JSON unchanged and `"template_id": "chrome-mcp"` returns a `job_id`. `get_job` follows the build: `progress.script` names the step the template's script has reached, such as `Google Chrome`, step 1 of 9, and `percent`. Once the job completes, `list_images` lists the image with its `image_id`.
4. `create_container` once for each desktop:

   ```json
   {"name": "web-test-1", "image": "IMAGE_ID", "parameters": {"CAPS": "network,storage"}}
   ```

   The template's compose asks for `CAPS`, the optional tool groups of Playwright MCP, and for `BROWSER_PROFILE`, a volume for the agent's Chrome profile. Leaving it out gives each container a volume of its own. `get_job` returns each `container_id` once the setup script has run.
5. Each container got an endpoint of its own, `/mcp/web-test-1` and so on, with every scope but `commands` and the desktop's `playwright` server. `create_mcp_endpoint_token` with `{"name": "web-test-1"}` returns its token once, and the agent hands each test agent one URL and one token. A test agent sees `screenshot`, `desktop_input`, `playwright__browser_navigate`, `playwright__browser_snapshot` and the rest, bound to its own desktop, and nothing that creates, changes or deletes a container.
6. Between test runs, `reset_container` returns a desktop to its image and keeps its volumes, so the Chrome profile and its sign-ins stay. When the work is over, `delete_container` with `"delete_volumes": true` removes each container with the volume made for it.

Several templates also go into one image, such as `vscode` with the `node` environment: **Merge and install** on the dashboard's **Templates** page does it, and `build_image` takes the scripts as `parts`, as [composes](MCP.md#ask-for-values-with-composes) describe.

## Test a web app in a real browser and hand back a recording

The request: "Check that sign-up works on my branch, and show me." The agent has the project on the machine it runs on and a container, `web-test-1`, from the image above, which has Chrome and Node.js.

1. It packs the project and asks for a one-time upload link: `create_transfer` with

   ```json
   {"operation": "container_upload", "container_id": "CONTAINER_ID", "destination": "/home/desktop", "name": "app.tar.gz"}
   ```

   and sends the file to the returned `url` with `curl --fail-with-body --upload-file app.tar.gz`. The link works once, for five minutes, and for this upload only.
2. `exec_container` unpacks it and installs the dependencies, as the desktop user:

   ```json
   {"container_id": "CONTAINER_ID", "timeout_seconds": 900, "script": "mkdir -p app\ntar -xzf app.tar.gz -C app\ncd app\nnpm ci"}
   ```

3. `launch_application` starts the development server in the background with `{"command": "npm", "args": ["run", "dev"], "cwd": "/home/desktop/app"}` and returns an `application_id`. `wait_for_condition` with that `application_id`, `"application_state": "running"` and `"log_contains": "Local:"` waits until the server says where it listens. If it never does, `application_logs` shows why.
4. `start_recording` with `{"fps": 2}` starts keeping frames of the desktop.
5. `launch_application` opens Chrome at the page with `{"command": "google-chrome", "args": ["http://localhost:5173/signup"]}`, and `wait_for_condition` with `"window_title": "Google Chrome"` waits for its window. From there the agent fills in the form with `act_and_observe`: each call clicks or types and returns the screenshot that follows, so the agent reads the page before its next step. It can use the desktop's Playwright server instead, which works by the page's structure rather than by pixels: `call_mcp_server_tool` with `{"container_id": "CONTAINER_ID", "server": "playwright", "tool": "browser_navigate", "arguments": {"url": "http://localhost:5173/signup"}}` calls one of its tools from `/mcp`, and `list_mcp_servers` with the `container_id` lists them with their arguments. An agent connected to the container's endpoint has the same tools as its own.
6. When something looks wrong, `save_screenshot` keeps that moment as a file in the desktop with a `download_url`.
7. `stop_recording` ends the recording and writes `recording.avi`, a video of it. `export_artifacts` packs the frames, the video and the server's log:

   ```json
   {"container_id": "CONTAINER_ID", "destination": "/home/desktop", "name": "signup-test.tar.gz", "recording_ids": ["RECORDING_ID"], "application_ids": ["SERVER_APPLICATION_ID"]}
   ```

8. The agent answers with what it found and the archive's `download_url`, which opens in the person's dashboard session. When it needs the archive itself, `create_transfer` with `container_download` gives it a link to fetch the file with curl.

`stop_application` ends the server, and `reset_container` gets the desktop ready for the next branch.

## Ask a person when a CAPTCHA appears

The request: "Download last month's invoice from the supplier's portal." The agent was given the endpoint of one desktop, `/mcp/web-test-1`, so every tool is bound to that container and takes no `container_id`.

1. `playwright__browser_navigate` opens the portal in the agent's Chrome, which appears on the desktop, and `playwright__browser_snapshot` shows a CAPTCHA where the sign-in form should be.
2. A CAPTCHA, a second sign-in factor or a consent window is for a person, so instead of trying to get past it the agent calls `request_human`:

   ```json
   {"message": "The supplier portal shows a CAPTCHA in Chrome. Please solve it and sign in, then press Done.", "wait_seconds": 120}
   ```

3. The dashboard shows the request on every page, with the container's name, the message and the endpoint it came through, and as a strip above that desktop. The person selects **Open desktop**, solves the CAPTCHA, signs in and presses **Done**, with a reply such as "signed in as accounts@".
4. The call returns `"status": "done"` with the `reply`. Had nobody answered within `wait_seconds`, it would have returned `"status": "pending"` with a `request_id`, and the agent would call `request_human` again with that `request_id` alone to keep waiting. `"declined"` means the person will not do it, and the agent stops and says so.
5. The answer says what the person did, not what the screen shows now, so the agent looks again with `screenshot` or `playwright__browser_snapshot` before it goes on.
6. It downloads the invoice in Chrome, finds the file with `list_files` in the folder Chrome saved it to, and `download_file` returns it with a `download_url` for the person.

Because the profile lives in the container's `BROWSER_PROFILE` volume, the session survives `reset_container`, and next month's invoice may need no person at all. **Activity**, under **MCP** on the dashboard, lists every call the agent made, the request included.
