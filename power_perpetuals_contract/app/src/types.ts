import { MethodsNamespace, IdlTypes, IdlAccounts } from "@coral-xyz/anchor";
import { Perpetuals } from "../../target/types/perpetuals";

export type PositionSide = "long" | "short";

export type Methods = MethodsNamespace<Perpetuals>;
export type Accounts = IdlAccounts<Perpetuals>;
export type Types = IdlTypes<Perpetuals>;

export type InitParams = any;

export type OracleParams = any;
export type PricingParams = any;
export type Permissions = any;
export type Fees = any;
export type BorrowRateParams = any;
export type TokenRatio = any;
export type SetCustomOraclePriceParams = any;
export type AmountAndFee = any;
export type NewPositionPricesAndFee = any;
export type PriceAndFee = any;
export type ProfitAndLoss = any;
export type SwapAmountAndFees = any;

export type Custody = any;
export type Pool = any;
export type Position = any;
export type PerpetualsAccount = any;