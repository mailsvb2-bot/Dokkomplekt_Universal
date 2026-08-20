// Generation preflight glue shared by manual one-document and batch creation.
// Business rules remain in dokkomplekt-core WorkflowPlan owners.

fn plan_selection_with_output_folder(
    documents: &[DocumentTemplateSpec],
    case: &SemanticCase,
    flags: &WorkflowFlags,
    folder_parts: &[FolderNamePart],
) -> WorkflowPlan {
    let mut planned_documents = documents.to_vec();
    if let Some(output_folder) =
        dokkomplekt_core::output_folder_requirement_document(case, folder_parts)
    {
        planned_documents.push(output_folder);
    }
    if planned_documents.len() == 1 {
        build_merged_popup_plan(&planned_documents[0], case, flags)
    } else {
        plan_workflow_batch(&planned_documents, case, flags)
    }
}
