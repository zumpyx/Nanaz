use mythic::{TaskMessage, TaskResponse};

pub fn handle(task: &TaskMessage) -> TaskResponse {
    TaskResponse::failed(
        task.id,
        "rpfwd must be handled by the runtime stream manager, not the command dispatcher",
    )
}
