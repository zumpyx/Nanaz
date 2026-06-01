// ps_new.js — process browser table renderer for the `ps` command.
//
// Patterned after Apollo's ps_new.js but trimmed: we omit the AV /
// admin-tool highlighting (different AV vendor in each engagement
// makes the list noisy) and the Steal-Token / Screenshot actions
// (those don't apply to a non-injected Linux/macOS agent). Kept is
// the right-click "Kill" action that hands off to the kill command.

function(task, responses) {
    if (task.status && task.status.includes("error")) {
        const combined = responses.reduce((prev, cur) => prev + cur, "");
        return {plaintext: combined || "ps failed"};
    }
    if (!responses || responses.length === 0) {
        return {plaintext: "No response yet from agent..."};
    }

    const headers = [
        {plaintext: "actions", type: "button", cellStyle: {}, width: 100, disableSort: true},
        {plaintext: "pid", type: "number", copyIcon: true, cellStyle: {}, width: 80},
        {plaintext: "ppid", type: "number", copyIcon: true, cellStyle: {}, width: 80},
        {plaintext: "arch", type: "string", cellStyle: {}, width: 80},
        {plaintext: "name", type: "string", cellStyle: {}, fillWidth: true},
        {plaintext: "user", type: "string", cellStyle: {}, width: 200},
        {plaintext: "command", type: "string", cellStyle: {}, fillWidth: true},
    ];

    const rows = [];
    for (let i = 0; i < responses.length; i++) {
        let data;
        try {
            data = JSON.parse(responses[i]);
        } catch (e) {
            return {plaintext: responses.join("")};
        }
        // Each response is a `processes` array.
        const procs = Array.isArray(data) ? data : (data["processes"] || []);
        for (const p of procs) {
            rows.push({
                pid: {plaintext: p["process_id"], cellStyle: {}, copyIcon: true},
                ppid: {plaintext: p["parent_process_id"], cellStyle: {}, copyIcon: true},
                arch: {plaintext: p["architecture"] || "", cellStyle: {}},
                name: {plaintext: p["name"] || "", cellStyle: {}},
                user: {plaintext: p["user"] || "", cellStyle: {}},
                command: {plaintext: p["command_line"] || "", cellStyle: {}},
                actions: {
                    button: {
                        name: "Actions",
                        type: "menu",
                        value: [
                            {
                                name: "More Info",
                                type: "dictionary",
                                value: {
                                    "Process Path": p["bin_path"] || "",
                                    "Command Line": p["command_line"] || "",
                                    "User": p["user"] || "",
                                    "Parent PID": p["parent_process_id"] || "",
                                },
                                leftColumnTitle: "Attribute",
                                rightColumnTitle: "Value",
                                title: "Information for " + (p["name"] || "process"),
                            },
                            {
                                name: "Kill",
                                type: "task",
                                startIcon: "kill",
                                ui_feature: "process_browser:kill",
                                parameters: {pid: p["process_id"]},
                            },
                        ],
                    },
                    cellStyle: {},
                },
            });
        }
    }

    return {table: [{headers: headers, rows: rows, title: ""}]};
}
