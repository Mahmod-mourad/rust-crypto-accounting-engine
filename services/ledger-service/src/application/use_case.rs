// Use cases will be implemented here as the business logic grows.
// Each use case is a struct with injected repository dependencies.
//
// Example pattern (not yet implemented):
//
// pub struct CreateAccountUseCase {
//     accounts: Arc<dyn AccountRepository>,
// }
//
// impl CreateAccountUseCase {
//     pub async fn execute(&self, cmd: CreateAccountCommand) -> AppResult<Account> {
//         // orchestration logic
//     }
// }
