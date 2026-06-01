// ls_new.js — file browser table renderer for the `ls` command.
//
// Patterned after Apollo's ls_new.js but simplified for the new Mythic
// UI. Inputs:
//   - task: the task object (status, parameters, etc.)
//   - responses: an array of strings; each string is a JSON-encoded
//     FileBrowserEntry from the agent.
//
// Output: a single table with one row per file/directory, plus an
// "Actions" menu per row that exposes the file_browser:download /
// file_browser:remove / cat / ls (drill in) actions.

function(task, responses) {
    if (task.status && task.status.includes("error")) {
        const combined = responses.reduce((prev, cur) => prev + cur, "");
        return {plaintext: combined || "ls failed"};
    }
    if (!responses || responses.length === 0) {
        return {plaintext: "No response yet from agent..."};
    }

    // ── Type maps (mirrors Apollo) ────────────────────────────
    const FileType = Object.freeze({
        ARCHIVE: "archive", DISKIMAGE: "diskimage",
        WORD: "word", EXCEL: "excel", POWERPOINT: "powerpoint",
        PDF: "pdf", DATABASE: "db", KEYMATERIAL: "keymaterial",
        SOURCECODE: "sourcecode", IMAGE: "image",
    });
    const fileExtensionMappings = new Map([
        [".zip", FileType.ARCHIVE], [".tar", FileType.ARCHIVE],
        [".gz", FileType.ARCHIVE], [".bz2", FileType.ARCHIVE],
        [".7z", FileType.ARCHIVE], [".rar", FileType.ARCHIVE],
        [".xz", FileType.ARCHIVE], [".zst", FileType.ARCHIVE],
        [".jar", FileType.ARCHIVE], [".war", FileType.ARCHIVE],

        [".dmg", FileType.DISKIMAGE], [".iso", FileType.DISKIMAGE],

        [".doc", FileType.WORD], [".docx", FileType.WORD],
        [".xls", FileType.EXCEL], [".xlsx", FileType.EXCEL], [".csv", FileType.EXCEL],
        [".ppt", FileType.POWERPOINT], [".pptx", FileType.POWERPOINT],
        [".pdf", FileType.PDF],
        [".db", FileType.DATABASE], [".sql", FileType.DATABASE],
        [".pem", FileType.KEYMATERIAL], [".cer", FileType.KEYMATERIAL],
        [".pfx", FileType.KEYMATERIAL], [".p12", FileType.KEYMATERIAL],

        [".ps1", FileType.SOURCECODE], [".vbs", FileType.SOURCECODE],
        [".js", FileType.SOURCECODE], [".py", FileType.SOURCECODE],
        [".rb", FileType.SOURCECODE], [".go", FileType.SOURCECODE],
        [".rs", FileType.SOURCECODE], [".c", FileType.SOURCECODE],
        [".cpp", FileType.SOURCECODE], [".h", FileType.SOURCECODE],
        [".hpp", FileType.SOURCECODE], [".cs", FileType.SOURCECODE],
        [".sh", FileType.SOURCECODE], [".bash", FileType.SOURCECODE],
        [".yaml", FileType.SOURCECODE], [".yml", FileType.SOURCECODE],
        [".xml", FileType.SOURCECODE], [".html", FileType.SOURCECODE],
        [".css", FileType.SOURCECODE], [".json", FileType.SOURCECODE],

        [".png", FileType.IMAGE], [".jpg", FileType.IMAGE],
        [".jpeg", FileType.IMAGE], [".gif", FileType.IMAGE],
        [".bmp", FileType.IMAGE], [".ico", FileType.IMAGE],
        [".webp", FileType.IMAGE], [".tiff", FileType.IMAGE], [".svg", FileType.IMAGE],
    ]);
    const fileStyleMap = new Map([
        [FileType.ARCHIVE, {startIcon: "archive", startIconColor: "goldenrod"}],
        [FileType.DISKIMAGE, {startIcon: "diskimage", startIconColor: "goldenrod"}],
        [FileType.WORD, {startIcon: "word", startIconColor: "cornflowerblue"}],
        [FileType.EXCEL, {startIcon: "excel", startIconColor: "darkseagreen"}],
        [FileType.POWERPOINT, {startIcon: "powerpoint", startIconColor: "indianred"}],
        [FileType.PDF, {startIcon: "pdf", startIconColor: "orangered"}],
        [FileType.DATABASE, {startIcon: "database"}],
        [FileType.KEYMATERIAL, {startIcon: "key"}],
        [FileType.SOURCECODE, {startIcon: "code", startIconColor: "rgb(25,142,117)"}],
        [FileType.IMAGE, {startIcon: "image"}],
    ]);

    function lookupEntryStyling(entry) {
        if (entry["is_file"]) {
            const ext = "." + (entry["name"].split(".").slice(-1)[0] || "");
            const cat = fileExtensionMappings.get(ext);
            const fallback = {startIcon: "", startIconColor: ""};
            return cat ? {...fallback, ...fileStyleMap.get(cat)} : fallback;
        }
        return {startIcon: "openFolder", startIconColor: "rgb(241,226,0)"};
    }

    function buildFullPath(parent, name) {
        if (!parent) return name;
        if (parent.endsWith("/") || parent.endsWith("\\")) return parent + name;
        return parent + "/" + name;
    }

    // Per-row actions menu.
    function actionsFor(data, entry) {
        const fullName = entry["full_name"] || buildFullPath(
            data["parent_path"] || "", data["name"] ? data["name"] : entry["name"]
        );
        if (entry["is_file"]) {
            return {
                name: "Actions",
                type: "menu",
                value: [
                    {
                        name: "View",
                        type: "task",
                        ui_feature: "cat",
                        parameters: {host: data["host"], full_path: fullName},
                    },
                    {
                        name: "Download",
                        type: "task",
                        ui_feature: "file_browser:download",
                        startIcon: "download",
                        parameters: {host: data["host"], full_path: fullName},
                    },
                    {
                        name: "Delete",
                        type: "task",
                        ui_feature: "file_browser:remove",
                        startIcon: "delete",
                        getConfirmation: true,
                        parameters: {host: data["host"], full_path: fullName},
                    },
                ],
            };
        }
        return {
            name: "Actions",
            type: "menu",
            value: [
                {
                    name: "List contents",
                    type: "task",
                    ui_feature: "file_browser:list",
                    startIcon: "list",
                    parameters: {host: data["host"], full_path: fullName},
                },
                {
                    name: "Delete",
                    type: "task",
                    ui_feature: "file_browser:remove",
                    startIcon: "delete",
                    getConfirmation: true,
                    parameters: {host: data["host"], full_path: fullName},
                },
            ],
        };
    }

    function makeRow(data, entry) {
        const styling = lookupEntryStyling(entry);
        const fullName = entry["full_name"] || buildFullPath(
            data["parent_path"] || "", data["name"] ? data["name"] : entry["name"]
        );
        return {
            name: {
                plaintext: entry["name"],
                startIcon: styling.startIcon,
                startIconColor: styling.startIconColor,
                cellStyle: {},
            },
            size: {plaintext: entry["size"], cellStyle: {}, width: 100},
            modify_time: {
                plaintext: entry["modify_time"]
                    ? new Date(entry["modify_time"]).toLocaleString()
                    : "",
                cellStyle: {},
            },
            actions: {button: actionsFor(data, entry), cellStyle: {}, width: 120},
        };
    }

    const headers = [
        {plaintext: "name", type: "string", fillWidth: true, cellStyle: {}},
        {plaintext: "size", type: "size", width: 100, cellStyle: {}},
        {plaintext: "modified", type: "string", fillWidth: true, cellStyle: {}},
        {plaintext: "actions", type: "button", width: 120, cellStyle: {}, disableSort: true},
    ];

    const rows = [];
    for (let i = 0; i < responses.length; i++) {
        let data;
        try {
            data = JSON.parse(responses[i]);
        } catch (e) {
            return {plaintext: responses.join("")};
        }
        if (data["is_file"]) {
            data["full_name"] = buildFullPath(
                data["parent_path"] || "", data["name"]
            );
            rows.push(makeRow(data, data));
        } else if (data["files"]) {
            for (const entry of data["files"]) {
                entry["full_name"] = buildFullPath(
                    buildFullPath(data["parent_path"] || "", data["name"] || ""),
                    entry["name"]
                );
                rows.push(makeRow(data, entry));
            }
        }
    }

    return {table: [{headers: headers, rows: rows, title: ""}]};
}
