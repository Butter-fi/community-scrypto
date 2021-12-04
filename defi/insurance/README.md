Here's the algo I'm thinking to implement:
1. The insurer should first deposit the XRD to cover future certificates
2. The insurer can deploy different certificates. The amount is limited by current deposits.
3. The insurer cannot withdraw deposits if those not cover more than 100% the certificates value (locked value).
4. Collected fees and cert purchases will increase the deposit so the Insurer can either withdraw some or create more certificates
5. A user should be registered first before purchasing certificate
6. A user should have accepted payback amount before withdraw
7. After the cert expiration the locked amount for this particular cert will be released, so the insurer can deploy more or withdraw
8. After the expiration the cert token can be burned.

Features:
- we can  add an approvals list (bad example, like in case of death so the approved person can receive the withdraw instead of a user). But then we should also register approvers. Or maybe we can use another naming for that person.
- extension method of existing certificate. No need to burn 
- insurer can do more deposits also


